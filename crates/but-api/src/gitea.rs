use anyhow::Result;
use but_api_macros::but_api;
use but_gitea::{AuthStatusResponse, AuthenticatedUser, json};
use but_secret::Sensitive;
use tracing::instrument;

/// Stores a Gitea Personal Access Token (PAT) for a self-hosted instance.
///
/// Validates and stores the provided PAT for a specific Gitea host, then returns
/// the authenticated user information.
///
/// # Arguments
///
/// * `access_token` - The Gitea PAT to store
/// * `host` - The Gitea instance URL, for example `https://gitea.company.com`
///
/// # Returns
///
/// * `Ok(AuthStatusResponse)` - Token is valid, contains user details and host
/// * `Err(_)` - If the token is invalid, host is unreachable, or storage fails
#[but_api(json::AuthStatusResponseSensitive)]
#[instrument(err(Debug))]
pub async fn store_gitea_selfhosted_pat(
    access_token: Sensitive<String>,
    host: String,
    view_host: Option<String>,
) -> Result<AuthStatusResponse> {
    let storage = but_forge_storage::Controller::from_path(but_path::app_data_dir()?);
    but_gitea::store_selfhosted_pat(&host, &access_token, &storage, view_host.as_deref()).await
}

/// Removes stored credentials for a specific Gitea account.
///
/// Deletes the access token associated with the specified Gitea account identifier.
///
/// # Arguments
///
/// * `account` - Identifier for the Gitea account
///
/// # Returns
///
/// * `Ok(())` - Always succeeds, even if no token was found
#[but_api]
#[instrument(err(Debug))]
pub fn forget_gitea_account(account: but_gitea::GiteaAccountIdentifier) -> Result<()> {
    let storage = but_forge_storage::Controller::from_path(but_path::app_data_dir()?);
    but_gitea::forget_gitea_access_token(&account, &storage).ok();
    Ok(())
}

/// Removes all stored Gitea credentials.
///
/// This signs out every configured Gitea account.
///
/// # Returns
///
/// * `Ok(())` - All tokens were cleared
/// * `Err(_)` - If storage cleanup fails
#[but_api]
#[instrument(err(Debug))]
pub fn clear_all_gitea_tokens() -> Result<()> {
    let storage = but_forge_storage::Controller::from_path(but_path::app_data_dir()?);
    but_gitea::clear_all_gitea_tokens(&storage)
}

/// Retrieves the authenticated user information for a Gitea account.
///
/// Returns `None` if no credentials are stored for the account.
///
/// # Arguments
///
/// * `account` - Identifier for the Gitea account to query
///
/// # Returns
///
/// * `Ok(Some(AuthenticatedUser))` - User information
/// * `Ok(None)` - No credentials stored for this account
/// * `Err(_)` - If the API request fails or credentials are invalid
#[but_api(json::AuthenticatedUserSensitive)]
#[instrument(err(Debug))]
pub async fn get_gitea_user(
    account: but_gitea::GiteaAccountIdentifier,
) -> Result<Option<AuthenticatedUser>> {
    let storage = but_forge_storage::Controller::from_path(but_path::app_data_dir()?);
    but_gitea::get_gitea_user(&account, &storage).await
}

/// Lists all Gitea accounts with stored credentials.
///
/// # Returns
///
/// * `Ok(Vec<GiteaAccountIdentifier>)` - List of all known accounts
/// * `Err(_)` - If storage access fails
#[but_api]
#[instrument(err(Debug))]
pub fn list_known_gitea_accounts() -> Result<Vec<but_gitea::GiteaAccountIdentifier>> {
    let storage = but_forge_storage::Controller::from_path(but_path::app_data_dir()?);
    but_gitea::list_known_gitea_accounts(&storage)
}

/// Validates stored Gitea credentials.
///
/// # Arguments
///
/// * `account` - Identifier for the Gitea account to validate
///
/// # Returns
///
/// * `Ok(CredentialCheckResult)` - Result indicating if credentials are valid
/// * `Err(_)` - If the validation request fails
#[but_api]
#[instrument(err(Debug))]
pub async fn check_gitea_credentials(
    account: but_gitea::GiteaAccountIdentifier,
) -> Result<but_gitea::CredentialCheckResult> {
    let storage = but_forge_storage::Controller::from_path(but_path::app_data_dir()?);
    but_gitea::check_credentials(&account, &storage).await
}
