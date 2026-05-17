use anyhow::{Context as _, Result};

use crate::client::GiteaClient;

/// List open pull requests.
pub async fn list(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    owner: &str,
    repo: &str,
    storage: &but_forge_storage::Controller,
) -> Result<Vec<crate::client::PullRequest>> {
    if let Ok(client) = GiteaClient::from_storage(storage, preferred_account) {
        client
            .list_open_pulls(owner, repo)
            .await
            .context("Failed to list open Gitea pull requests")
    } else {
        Ok(vec![])
    }
}

pub async fn list_recently_closed(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    owner: &str,
    repo: &str,
    storage: &but_forge_storage::Controller,
) -> Result<Vec<crate::client::PullRequest>> {
    GiteaClient::from_storage(storage, preferred_account)?
        .list_recently_closed_pulls(owner, repo)
        .await
        .context("Failed to list recently closed Gitea pull requests")
}

/// List pull requests for a target branch.
pub async fn list_all_for_target(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    owner: &str,
    repo: &str,
    target_branch: &str,
    storage: &but_forge_storage::Controller,
) -> Result<Vec<crate::client::PullRequest>> {
    if let Ok(client) = GiteaClient::from_storage(storage, preferred_account) {
        client
            .list_pulls_for_target(owner, repo, target_branch)
            .await
            .context("Failed to list Gitea pull requests for target branch")
    } else {
        Ok(vec![])
    }
}

/// Create a pull request.
pub async fn create(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    params: crate::client::CreatePullRequestParams<'_>,
    storage: &but_forge_storage::Controller,
) -> Result<crate::client::PullRequest> {
    let pr = GiteaClient::from_storage(storage, preferred_account)?
        .create_pull_request(&params)
        .await
        .context("Failed to create Gitea pull request")?;
    Ok(pr)
}

/// Get a pull request.
pub async fn get(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    owner: &str,
    repo: &str,
    pr_number: usize,
    storage: &but_forge_storage::Controller,
) -> Result<crate::client::PullRequest> {
    let pr_number = pr_number.try_into().context("PR number is too large")?;
    let pr = GiteaClient::from_storage(storage, preferred_account)?
        .get_pull_request(owner, repo, pr_number)
        .await
        .context("Failed to get Gitea pull request")?;
    Ok(pr)
}

/// Update a pull request.
pub async fn update(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    params: crate::client::UpdatePullRequestParams<'_>,
    storage: &but_forge_storage::Controller,
) -> Result<crate::client::PullRequest> {
    let pr = GiteaClient::from_storage(storage, preferred_account)?
        .update_pull_request(&params)
        .await
        .context("Failed to update Gitea pull request")?;
    Ok(pr)
}

/// Merge a pull request.
pub async fn merge(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    params: crate::client::MergePullRequestParams<'_>,
    storage: &but_forge_storage::Controller,
) -> Result<()> {
    GiteaClient::from_storage(storage, preferred_account)?
        .merge_pull_request(&params)
        .await
        .context("Failed to merge Gitea pull request")
}

/// Set the pull request draft state.
pub async fn set_draft_state(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    params: crate::client::SetPullRequestDraftStateParams<'_>,
    storage: &but_forge_storage::Controller,
) -> Result<()> {
    GiteaClient::from_storage(storage, preferred_account)?
        .set_pull_request_draft_state(&params)
        .await
        .context("Failed to update Gitea pull request draft state")
}
