use anyhow::{Context as _, Result};
use but_secret::Sensitive;
use serde::Serialize;

mod client;
pub mod checks;
pub mod pr;
mod repo;
mod token;

pub use client::{
    AuthenticatedUser as GiteaApiAuthenticatedUser, CreatePullRequestParams, GiteaClient,
    GiteaCommitStatus, GiteaLabel, GiteaRepoPermissions, GiteaRepository, GiteaUser,
    HttpStatusError, MergePullRequestParams, PullRequest, SetPullRequestDraftStateParams,
    UpdatePullRequestParams, normalize_host,
};
pub use repo::fetch_repo;
pub use token::GiteaAccountIdentifier;

/// The result of storing a Gitea access token.
#[derive(Debug, Clone)]
pub struct AuthStatusResponse {
    /// The access token.
    pub access_token: Sensitive<String>,
    /// The Gitea username.
    pub username: String,
    /// The user's display name, if available.
    pub name: Option<String>,
    /// The user's email address, if available.
    pub email: Option<String>,
    /// The normalized Gitea API instance URL.
    pub host: String,
    /// Optional web UI base URL for browser links.
    pub view_host: Option<String>,
}

/// Store a self-hosted Gitea PAT and fetch the associated user data.
pub async fn store_selfhosted_pat(
    host: &str,
    access_token: &Sensitive<String>,
    storage: &but_forge_storage::Controller,
    view_host: Option<&str>,
) -> Result<AuthStatusResponse> {
    let normalized_host = normalize_host(host).context("Failed to normalize Gitea host")?;
    let normalized_view_host = view_host
        .map(normalize_host)
        .transpose()
        .context("Failed to normalize Gitea view host")?;
    let user = fetch_and_persist_selfhosted_user_data(
        &normalized_host,
        normalized_view_host.as_deref(),
        access_token,
        storage,
    )
    .await?;
    Ok(AuthStatusResponse {
        access_token: access_token.clone(),
        username: user.username,
        name: user.name,
        email: user.email,
        host: normalized_host,
        view_host: normalized_view_host,
    })
}

/// Cache the user profile so it's available offline.
fn cache_user_profile(
    account: &GiteaAccountIdentifier,
    user: &client::AuthenticatedUser,
    storage: &but_forge_storage::Controller,
) {
    let profile = but_forge_storage::settings::CachedProfile {
        avatar_url: user.avatar_url.clone(),
        name: user.name.clone(),
        email: user.email.clone(),
    };
    let key = account.cache_key();
    let existing = storage.cached_profile(&key).ok().flatten();
    if existing.as_ref() == Some(&profile) {
        return;
    }
    if let Err(err) = storage.set_cached_profile(&key, Some(profile)) {
        tracing::warn!(?account, "Failed to update cached Gitea profile: {err}");
    }
}

/// Fetch the authenticated user data from Gitea and persist the access token.
async fn fetch_and_persist_selfhosted_user_data(
    host: &str,
    view_host: Option<&str>,
    access_token: &Sensitive<String>,
    storage: &but_forge_storage::Controller,
) -> Result<client::AuthenticatedUser, anyhow::Error> {
    let client = client::GiteaClient::new_with_host(access_token, host)
        .context("Failed to create Gitea client")?;
    let user = client
        .get_authenticated()
        .await
        .context("Failed to get authenticated Gitea user")?;
    let account_id = token::GiteaAccountIdentifier::selfhosted_with_view_host(
        &user.username,
        host,
        view_host.map(str::to_string),
    );
    token::persist_gitea_access_token(&account_id, access_token, storage)
        .context("Failed to persist access token")?;
    cache_user_profile(&account_id, &user, storage);
    Ok(user)
}

/// Remove stored credentials for a Gitea account.
pub fn forget_gitea_access_token(
    account: &GiteaAccountIdentifier,
    storage: &but_forge_storage::Controller,
) -> Result<()> {
    token::delete_gitea_access_token(account, storage).context("Failed to delete access token")
}

/// Return the authenticated Gitea user for an account.
pub async fn get_gitea_user(
    account: &GiteaAccountIdentifier,
    storage: &but_forge_storage::Controller,
) -> Result<Option<AuthenticatedUser>> {
    if let Some(access_token) = token::get_gitea_access_token(account, storage)? {
        let client = account
            .client(&access_token)
            .context("Failed to create Gitea client")?;
        match client.get_authenticated().await {
            Ok(user) => {
                cache_user_profile(account, &user, storage);
                Ok(Some(AuthenticatedUser {
                    access_token,
                    username: user.username,
                    avatar_url: user.avatar_url,
                    name: user.name,
                    email: user.email,
                }))
            }
            Err(client_err) => {
                let cache_key = account.cache_key();
                if let Some(reqwest_err) = client_err.downcast_ref::<reqwest::Error>()
                    && is_network_error(reqwest_err)
                {
                    match storage.cached_profile(&cache_key) {
                        Ok(Some(cached)) => {
                            return Ok(Some(AuthenticatedUser {
                                access_token,
                                username: account.username().to_owned(),
                                avatar_url: cached.avatar_url,
                                name: cached.name,
                                email: cached.email,
                            }));
                        }
                        Ok(None) => {}
                        Err(err) => {
                            tracing::warn!("Failed to read cached Gitea profile: {err}");
                        }
                    }
                    return Err(client_err.context(but_error::Context::new_static(
                        but_error::Code::NetworkError,
                        "Unable to connect to Gitea.",
                    )));
                }
                if let Some(http_err) = client_err.downcast_ref::<client::HttpStatusError>()
                    && matches!(
                        http_err.status,
                        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
                    )
                    && let Err(err) = storage.set_cached_profile(&cache_key, None)
                {
                    tracing::warn!("Failed to clear cached Gitea profile: {err}");
                }
                Err(client_err.context("Failed to get authenticated Gitea user"))
            }
        }
    } else {
        Ok(None)
    }
}

/// Check if an error is a network connectivity error.
fn is_network_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request()
}

/// Credential validity for a stored Gitea account.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum CredentialCheckResult {
    /// The token is present and accepted by Gitea.
    Valid,
    /// The token is present but rejected by Gitea.
    Invalid,
    /// No token is stored for the account.
    NoCredentials,
}

/// Check the validity of the stored credentials for the given Gitea account.
pub async fn check_credentials(
    account: &GiteaAccountIdentifier,
    storage: &but_forge_storage::Controller,
) -> Result<CredentialCheckResult> {
    if let Some(access_token) = token::get_gitea_access_token(account, storage)? {
        let client = account
            .client(&access_token)
            .context("Failed to create Gitea client")?;
        match client.get_authenticated().await {
            Ok(_) => Ok(CredentialCheckResult::Valid),
            Err(client_err) => {
                if let Some(reqwest_err) = client_err.downcast_ref::<reqwest::Error>()
                    && is_network_error(reqwest_err)
                {
                    return Err(client_err.context(but_error::Context::new_static(
                        but_error::Code::NetworkError,
                        "Unable to connect to Gitea.",
                    )));
                }
                Ok(CredentialCheckResult::Invalid)
            }
        }
    } else {
        Ok(CredentialCheckResult::NoCredentials)
    }
}

/// List all known Gitea accounts.
pub fn list_known_gitea_accounts(
    storage: &but_forge_storage::Controller,
) -> Result<Vec<token::GiteaAccountIdentifier>> {
    token::list_known_gitea_accounts(storage).context("Failed to list known Gitea accounts")
}

/// Clear all stored Gitea tokens.
pub fn clear_all_gitea_tokens(storage: &but_forge_storage::Controller) -> Result<()> {
    token::clear_all_gitea_accounts(storage).context("Failed to clear all Gitea tokens")
}

/// An authenticated Gitea user with credentials.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    /// The access token.
    pub access_token: Sensitive<String>,
    /// The Gitea username.
    pub username: String,
    /// URL to the user's avatar image, if available.
    pub avatar_url: Option<String>,
    /// The user's display name, if available.
    pub name: Option<String>,
    /// The user's email address, if available.
    pub email: Option<String>,
}

/// JSON serialization types for Gitea API responses.
pub mod json {
    use serde::Serialize;

    use crate::{AuthStatusResponse, AuthenticatedUser};

    /// Serializable version of [`AuthStatusResponse`] with exposed access token.
    #[derive(Debug, Serialize)]
    #[cfg_attr(feature = "export-schema", derive(schemars::JsonSchema))]
    #[cfg_attr(
        feature = "export-schema",
        schemars(rename = "GiteaAuthStatusResponseSensitive")
    )]
    #[serde(rename_all = "camelCase")]
    pub struct AuthStatusResponseSensitive {
        /// The Gitea access token as a plain string.
        pub access_token: String,
        /// The Gitea username.
        pub username: String,
        /// The user's display name, if available.
        pub name: Option<String>,
        /// The user's email address, if available.
        pub email: Option<String>,
        /// The normalized Gitea API instance URL.
        pub host: String,
        /// Optional web UI base URL for browser links.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub view_host: Option<String>,
    }

    impl From<AuthStatusResponse> for AuthStatusResponseSensitive {
        fn from(
            AuthStatusResponse {
                access_token,
                username,
                name,
                email,
                host,
                view_host,
            }: AuthStatusResponse,
        ) -> Self {
            AuthStatusResponseSensitive {
                access_token: access_token.0,
                username,
                name,
                email,
                host,
                view_host,
            }
        }
    }

    #[cfg(feature = "export-schema")]
    but_schemars::register_sdk_type!(AuthStatusResponseSensitive);

    /// Serializable version of [`AuthenticatedUser`] with exposed access token.
    #[derive(Debug, Serialize)]
    #[cfg_attr(feature = "export-schema", derive(schemars::JsonSchema))]
    #[cfg_attr(
        feature = "export-schema",
        schemars(rename = "GiteaAuthenticatedUserSensitive")
    )]
    #[serde(rename_all = "camelCase")]
    pub struct AuthenticatedUserSensitive {
        /// The Gitea access token as a plain string.
        pub access_token: String,
        /// The Gitea username.
        pub username: String,
        /// URL to the user's avatar image, if available.
        pub avatar_url: Option<String>,
        /// The user's display name, if available.
        pub name: Option<String>,
        /// The user's email address, if available.
        pub email: Option<String>,
    }

    impl From<AuthenticatedUser> for AuthenticatedUserSensitive {
        fn from(
            AuthenticatedUser {
                access_token,
                username,
                avatar_url,
                name,
                email,
            }: AuthenticatedUser,
        ) -> Self {
            AuthenticatedUserSensitive {
                access_token: access_token.0,
                username,
                avatar_url,
                name,
                email,
            }
        }
    }

    #[cfg(feature = "export-schema")]
    but_schemars::register_sdk_type!(AuthenticatedUserSensitive);
}
