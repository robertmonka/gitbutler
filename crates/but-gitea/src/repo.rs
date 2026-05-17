use anyhow::{Context as _, Result};

pub async fn fetch_repo(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    owner: &str,
    repo: &str,
    storage: &but_forge_storage::Controller,
) -> Result<crate::client::GiteaRepository> {
    crate::client::GiteaClient::from_storage(storage, preferred_account)?
        .get_repo(owner, repo)
        .await
        .context("Failed to fetch Gitea repository")
}
