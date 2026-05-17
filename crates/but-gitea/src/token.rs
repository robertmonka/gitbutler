use std::sync::Mutex;

use anyhow::Result;
use but_secret::{Sensitive, secret};
use serde::{Deserialize, Serialize};

use crate::client::{GiteaClient, normalize_host};

/// Persist a Gitea account access token securely.
pub fn persist_gitea_access_token(
    account_id: &GiteaAccountIdentifier,
    access_token: &Sensitive<String>,
    storage: &but_forge_storage::Controller,
) -> Result<()> {
    let account = GiteaAccount::new(account_id, access_token.clone());
    persist_gitea_account(&account, storage)
}

/// Delete a Gitea account access token.
pub fn delete_gitea_access_token(
    account_id: &GiteaAccountIdentifier,
    storage: &but_forge_storage::Controller,
) -> Result<()> {
    delete_gitea_account(account_id, storage)
}

/// Retrieve a Gitea account access token.
pub fn get_gitea_access_token(
    account_id: &GiteaAccountIdentifier,
    storage: &but_forge_storage::Controller,
) -> Result<Option<Sensitive<String>>> {
    let account = find_gitea_account(account_id, storage)?;
    Ok(account.map(|account| account.access_token()))
}

/// List all known Gitea accounts.
pub fn list_known_gitea_accounts(
    storage: &but_forge_storage::Controller,
) -> Result<Vec<GiteaAccountIdentifier>> {
    Ok(storage
        .gitea_accounts()?
        .iter()
        .map(Into::into)
        .collect::<Vec<_>>())
}

/// Clear all known Gitea accounts and their secrets.
pub fn clear_all_gitea_accounts(storage: &but_forge_storage::Controller) -> Result<()> {
    delete_all_gitea_accounts(storage)?;
    Ok(())
}

/// A stored Gitea account identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "export-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", tag = "type", content = "info")]
pub enum GiteaAccountIdentifier {
    /// A PAT for a self-hosted Gitea instance.
    SelfHosted {
        /// The Gitea username.
        username: String,
        /// The normalized Gitea API instance URL.
        host: String,
        /// Optional web UI base URL for browser links when it differs from [`host`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view_host: Option<String>,
    },
}
#[cfg(feature = "export-schema")]
but_schemars::register_sdk_type!(GiteaAccountIdentifier);

impl GiteaAccountIdentifier {
    /// Create a self-hosted Gitea account identifier.
    pub fn selfhosted(username: &str, host: &str) -> Self {
        Self::selfhosted_with_view_host(username, host, None)
    }

    /// Create a self-hosted Gitea account identifier with separate web UI URL.
    pub fn selfhosted_with_view_host(
        username: &str,
        host: &str,
        view_host: Option<String>,
    ) -> Self {
        GiteaAccountIdentifier::SelfHosted {
            username: username.to_string(),
            host: normalize_host(host).unwrap_or_else(|_| host.to_string()),
            view_host: view_host
                .as_deref()
                .and_then(|view_host| normalize_host(view_host).ok()),
        }
    }

    /// Return the account username.
    pub fn username(&self) -> &str {
        match self {
            GiteaAccountIdentifier::SelfHosted { username, .. } => username,
        }
    }

    /// Return the normalized API instance URL.
    pub fn host(&self) -> &str {
        match self {
            GiteaAccountIdentifier::SelfHosted { host, .. } => host,
        }
    }

    /// Return the normalized web UI base URL, if configured.
    pub fn view_host(&self) -> Option<&str> {
        match self {
            GiteaAccountIdentifier::SelfHosted { view_host, .. } => view_host.as_deref(),
        }
    }

    /// The key used to store and look up the cached profile for this account.
    pub fn cache_key(&self) -> String {
        match self {
            GiteaAccountIdentifier::SelfHosted { username, host, .. } => {
                format!("gitea_selfhosted_{username}@{host}")
            }
        }
    }

    /// Create a Gitea client for this account.
    pub fn client(&self, access_token: &Sensitive<String>) -> Result<GiteaClient> {
        match self {
            GiteaAccountIdentifier::SelfHosted { host, .. } => {
                GiteaClient::new_with_host(access_token, host)
            }
        }
    }

    /// Retrieve the custom forge host.
    pub fn custom_host(&self) -> Option<String> {
        Some(self.host().to_string())
    }

    /// Whether the API or web UI URL contains the `gitea` hostname marker.
    pub fn hosts_contain_gitea_keyword(&self) -> bool {
        match self {
            GiteaAccountIdentifier::SelfHosted {
                host, view_host, ..
            } => hosts_contain_gitea_keyword(host, view_host.as_deref()),
        }
    }
}

/// Returns true when either the API or view URL contains `gitea`.
pub fn hosts_contain_gitea_keyword(api_host: &str, view_host: Option<&str>) -> bool {
    api_host.contains("gitea") || view_host.is_some_and(|host| host.contains("gitea"))
}

impl std::fmt::Display for GiteaAccountIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GiteaAccountIdentifier::SelfHosted { username, host, .. } => {
                write!(f, "Self-hosted {username}@{host}")
            }
        }
    }
}

/// A Gitea account with its access token.
pub enum GiteaAccount {
    /// A self-hosted Gitea account.
    SelfHosted {
        /// The Gitea username.
        username: String,
        /// The normalized Gitea API instance URL.
        host: String,
        /// Optional web UI base URL for browser links.
        view_host: Option<String>,
        /// The account access token.
        access_token: Sensitive<String>,
    },
}

impl From<&GiteaAccount> for but_forge_storage::settings::GiteaAccount {
    fn from(account: &GiteaAccount) -> Self {
        let access_token_key = account.secret_key();
        match account {
            GiteaAccount::SelfHosted {
                host,
                view_host,
                username,
                ..
            } => but_forge_storage::settings::GiteaAccount::SelfHosted {
                username: username.to_owned(),
                host: host.to_owned(),
                view_host: view_host.clone(),
                access_token_key,
            },
        }
    }
}

impl From<&but_forge_storage::settings::GiteaAccount> for GiteaAccountIdentifier {
    fn from(account: &but_forge_storage::settings::GiteaAccount) -> Self {
        match account {
            but_forge_storage::settings::GiteaAccount::SelfHosted {
                host,
                view_host,
                username,
                ..
            } => GiteaAccountIdentifier::SelfHosted {
                username: username.to_owned(),
                host: host.to_owned(),
                view_host: view_host.clone(),
            },
        }
    }
}

impl GiteaAccount {
    /// Create a token-bearing account from an identifier.
    pub fn new(account_id: &GiteaAccountIdentifier, access_token: Sensitive<String>) -> Self {
        match account_id {
            GiteaAccountIdentifier::SelfHosted {
                username,
                host,
                view_host,
            } => GiteaAccount::SelfHosted {
                username: username.to_owned(),
                host: host.to_owned(),
                view_host: view_host.clone(),
                access_token,
            },
        }
    }

    fn secret_key(&self) -> String {
        match self {
            GiteaAccount::SelfHosted {
                host,
                view_host,
                username,
                ..
            } => {
                GiteaAccountIdentifier::selfhosted_with_view_host(username, host, view_host.clone())
                    .cache_key()
            }
        }
    }

    fn secret_value(&self) -> Result<Sensitive<String>> {
        Ok(self.access_token())
    }

    fn access_token(&self) -> Sensitive<String> {
        match self {
            GiteaAccount::SelfHosted { access_token, .. } => access_token.clone(),
        }
    }
}

fn retrieve_gitea_secret(account_secret_key: &str) -> Result<Option<Sensitive<String>>> {
    static FAIR_QUEUE: Mutex<()> = Mutex::new(());
    let _one_at_a_time_to_prevent_races = FAIR_QUEUE.lock().unwrap();
    secret::retrieve(account_secret_key, secret::Namespace::BuildKind)
}

fn persist_gitea_account(
    account: &GiteaAccount,
    storage: &but_forge_storage::Controller,
) -> Result<()> {
    let secret_key = account.secret_key();
    storage.add_gitea_account(&account.into())?;

    static FAIR_QUEUE: Mutex<()> = Mutex::new(());
    let _one_at_a_time_to_prevent_races = FAIR_QUEUE.lock().unwrap();
    secret::persist(
        &secret_key,
        &account.secret_value()?,
        secret::Namespace::BuildKind,
    )
}

fn delete_gitea_account(
    account: &GiteaAccountIdentifier,
    storage: &but_forge_storage::Controller,
) -> Result<()> {
    let secret_key = account.cache_key();
    let storage_account = match account {
        GiteaAccountIdentifier::SelfHosted {
            host,
            view_host,
            username,
        } => but_forge_storage::settings::GiteaAccount::SelfHosted {
            username: username.to_owned(),
            host: host.to_owned(),
            view_host: view_host.clone(),
            access_token_key: secret_key.clone(),
        },
    };
    storage.remove_gitea_account(&storage_account)?;

    static FAIR_QUEUE: Mutex<()> = Mutex::new(());
    let _one_at_a_time_to_prevent_races = FAIR_QUEUE.lock().unwrap();
    secret::delete(&secret_key, secret::Namespace::BuildKind)
}

fn delete_all_gitea_accounts(storage: &but_forge_storage::Controller) -> Result<()> {
    let keys_to_delete = storage.clear_all_gitea_accounts()?;
    static FAIR_QUEUE: Mutex<()> = Mutex::new(());
    let _one_at_a_time_to_prevent_races = FAIR_QUEUE.lock().unwrap();
    for key in keys_to_delete {
        secret::delete(&key, secret::Namespace::BuildKind)?;
    }
    Ok(())
}

fn find_gitea_account(
    account_id: &GiteaAccountIdentifier,
    storage: &but_forge_storage::Controller,
) -> Result<Option<GiteaAccount>> {
    let accounts = storage.gitea_accounts()?;
    let result = match account_id {
        GiteaAccountIdentifier::SelfHosted { username, host, .. } => {
            accounts.iter().find_map(|account| {
                if let but_forge_storage::settings::GiteaAccount::SelfHosted {
                    username: account_username,
                    host: account_host,
                    view_host: account_view_host,
                    access_token_key,
                } = account
                    && account_host == host
                    && account_username == username
                    && let Some(access_token) =
                        retrieve_gitea_secret(access_token_key).ok().flatten()
                {
                    return Some(GiteaAccount::SelfHosted {
                        username: account_username.clone(),
                        host: account_host.clone(),
                        view_host: account_view_host.clone(),
                        access_token,
                    });
                }
                None
            })
        }
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::GiteaAccountIdentifier;

    #[test]
    fn cache_key_separates_accounts_on_same_host_by_username() {
        let alice = GiteaAccountIdentifier::selfhosted("alice", "https://gitea.example.com");
        let bob = GiteaAccountIdentifier::selfhosted("bob", "https://gitea.example.com");

        assert_ne!(alice.cache_key(), bob.cache_key());
        assert_eq!(
            alice.cache_key(),
            "gitea_selfhosted_alice@https://gitea.example.com"
        );
    }
}
