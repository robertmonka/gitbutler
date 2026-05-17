use anyhow::{Context as _, Result};

use crate::client::GiteaClient;

pub async fn list_for_ref(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    owner: &str,
    repo: &str,
    reference: &str,
    storage: &but_forge_storage::Controller,
) -> Result<Option<Vec<crate::client::GiteaCommitStatus>>> {
    GiteaClient::from_storage(storage, preferred_account)?
        .list_checks_for_ref(owner, repo, reference)
        .await
        .context("Failed to list Gitea commit statuses for ref")
}
