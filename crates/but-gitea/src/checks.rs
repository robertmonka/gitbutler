use anyhow::{Context as _, Result};

use crate::client::GiteaClient;

/// Fetch commit statuses for a branch, tag, or SHA.
///
/// Returns `None` when Gitea cannot resolve the ref (HTTP 400/404/422) — the
/// branch may not have propagated yet after a push, or a reverse proxy may
/// reject `%2F` in path segments for slash branches. That is an expected
/// "no checks" state for display; because it can be transient the caller must
/// not let it overwrite a cached result. A resolvable ref returns
/// `Some(statuses)`, where an empty vec is authoritative "no checks".
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
