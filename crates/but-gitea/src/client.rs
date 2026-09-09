use anyhow::{Context, Result, bail};
use but_secret::Sensitive;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};
use url::Url;

const GITEA_API_PATH: &str = "/api/v1";

/// An HTTP error with a status code.
#[derive(Debug, thiserror::Error)]
#[error("HTTP {status}")]
pub struct HttpStatusError {
    /// The HTTP status code.
    pub status: reqwest::StatusCode,
}

/// A Gitea API client.
pub struct GiteaClient {
    client: reqwest::Client,
    base_url: String,
}

impl GiteaClient {
    /// Create a new Gitea client for a host.
    pub fn new_with_host(access_token: &Sensitive<String>, host: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("gb-gitea-integration"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("token {}", access_token.0))?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            client,
            base_url: api_base_url(host)?,
        })
    }

    /// Create a new client from stored credentials.
    pub fn from_storage(
        storage: &but_forge_storage::Controller,
        preferred_account: Option<&crate::GiteaAccountIdentifier>,
    ) -> anyhow::Result<Self> {
        let account_id = resolve_account(preferred_account, storage)?;
        if let Some(access_token) = crate::token::get_gitea_access_token(&account_id, storage)? {
            account_id.client(&access_token)
        } else {
            Err(anyhow::anyhow!(
                "No Gitea access token found for account '{account_id}'.\nRun 'but config forge auth' to re-authenticate."
            ))
        }
    }

    /// Get the authenticated user.
    pub async fn get_authenticated(&self) -> Result<AuthenticatedUser> {
        #[derive(Deserialize)]
        struct User {
            login: String,
            full_name: Option<String>,
            email: Option<String>,
            avatar_url: Option<String>,
        }

        let url = format!("{}/user", self.base_url);
        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(HttpStatusError {
                status: response.status(),
            }
            .into());
        }

        let user: User = response.json().await?;
        Ok(AuthenticatedUser {
            username: user.login,
            avatar_url: user.avatar_url,
            name: user.full_name,
            email: user.email,
        })
    }

    pub async fn get_repo(&self, owner: &str, repo: &str) -> Result<GiteaRepository> {
        #[derive(Deserialize)]
        struct ApiPermissions {
            #[serde(default)]
            admin: bool,
            #[serde(default)]
            push: bool,
            #[serde(default)]
            pull: bool,
        }

        #[derive(Deserialize)]
        struct ApiRepository {
            #[serde(default)]
            permissions: Option<ApiPermissions>,
            #[serde(default)]
            fork: bool,
            #[serde(default)]
            private: bool,
            #[serde(default)]
            default_delete_branch_after_merge: Option<bool>,
        }

        let url = format!("{}/repos/{}/{}", self.base_url, owner, repo);
        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("Failed to get Gitea repo: {status} - {error_text}");
        }
        let repo: ApiRepository = response.json().await?;
        Ok(GiteaRepository {
            permissions: repo.permissions.map(|p| GiteaRepoPermissions {
                admin: p.admin,
                maintain: p.push,
                push: p.push,
                triage: p.push,
                pull: p.pull,
            }),
            fork: repo.fork,
            private: repo.private,
            delete_branch_on_merge: repo.default_delete_branch_after_merge,
        })
    }

    pub async fn list_checks_for_ref(
        &self,
        owner: &str,
        repo: &str,
        reference: &str,
    ) -> Result<Option<Vec<GiteaCommitStatus>>> {
        let sha = match self.resolve_commit_sha(owner, repo, reference).await? {
            Some(sha) => sha,
            None => return Ok(None),
        };
        self.list_statuses_for_sha(owner, repo, &sha).await
    }

    /// Resolve a branch, tag, or SHA to a commit SHA.
    ///
    /// Branch names with slashes cannot go in the `/commits/{ref}/statuses` path:
    /// Gitea (and typical reverse proxies) reject `%2F` there with HTTP 400.
    /// The commits list endpoint takes `sha` as a query parameter, which proxies
    /// accept, and then statuses are fetched with the hex SHA.
    async fn resolve_commit_sha(
        &self,
        owner: &str,
        repo: &str,
        reference: &str,
    ) -> Result<Option<String>> {
        if is_likely_commit_sha(reference) {
            return Ok(Some(reference.to_owned()));
        }

        #[derive(Deserialize)]
        struct ApiCommit {
            sha: String,
        }

        let url = format!("{}/repos/{owner}/{repo}/commits", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&[("sha", reference), ("limit", "1")])
            .send()
            .await?;
        let status = response.status();
        if is_unresolvable_ref_status(status) {
            return Ok(None);
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            bail!("Failed to resolve Gitea commit for ref: {status} - {error_text}");
        }

        let commits: Vec<ApiCommit> = response.json().await?;
        Ok(commits.into_iter().next().map(|commit| commit.sha))
    }

    async fn list_statuses_for_sha(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
    ) -> Result<Option<Vec<GiteaCommitStatus>>> {
        #[derive(Deserialize)]
        struct ApiStatus {
            id: i64,
            #[serde(default)]
            context: String,
            #[serde(default)]
            description: Option<String>,
            #[serde(default)]
            status: String,
            #[serde(default)]
            target_url: Option<String>,
            #[serde(default)]
            url: Option<String>,
            #[serde(default)]
            created_at: Option<String>,
            #[serde(default)]
            updated_at: Option<String>,
        }

        let url = format!(
            "{}/repos/{owner}/{repo}/commits/{}/statuses",
            self.base_url,
            urlencoding::encode(sha)
        );
        let response = self.client.get(&url).send().await?;
        let status = response.status();
        if is_unresolvable_ref_status(status) {
            return Ok(None);
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            bail!("Failed to list Gitea commit statuses: {status} - {error_text}");
        }

        let statuses: Vec<ApiStatus> = response.json().await?;
        Ok(Some(
            statuses
                .into_iter()
                .map(|s| GiteaCommitStatus {
                    id: s.id,
                    context: s.context,
                    description: s.description,
                    status: s.status,
                    target_url: s.target_url,
                    url: s.url,
                    created_at: s.created_at,
                    updated_at: s.updated_at,
                    head_sha: sha.to_owned(),
                })
                .collect(),
        ))
    }

    /// List open pull requests for a repository.
    pub async fn list_open_pulls(&self, owner: &str, repo: &str) -> Result<Vec<PullRequest>> {
        self.list_pulls(owner, repo, "open").await
    }

    pub async fn list_recently_closed_pulls(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<PullRequest>> {
        let url = format!("{}/repos/{}/{}/pulls", self.base_url, owner, repo);
        let response = self
            .client
            .get(&url)
            .query(&[
                ("state", "closed"),
                ("sort", "recentupdate"),
                ("page", "1"),
                ("limit", "50"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("Failed to list closed Gitea pull requests: {status} - {error_text}");
        }

        let pulls: Vec<GiteaPullRequest> = response.json().await?;
        Ok(pulls.into_iter().map(Into::into).collect())
    }

    /// List pull requests targeting a base branch.
    pub async fn list_pulls_for_target(
        &self,
        owner: &str,
        repo: &str,
        target_branch: &str,
    ) -> Result<Vec<PullRequest>> {
        let prs = self.list_pulls(owner, repo, "all").await?;
        Ok(prs
            .into_iter()
            .filter(|pr| pr.target_branch == target_branch)
            .collect())
    }

    async fn list_pulls(&self, owner: &str, repo: &str, state: &str) -> Result<Vec<PullRequest>> {
        let url = format!("{}/repos/{}/{}/pulls", self.base_url, owner, repo);
        let mut page = 1;
        let limit = 50;
        let mut pulls = Vec::new();

        loop {
            let response = self
                .client
                .get(&url)
                .query(&[
                    ("state", state.to_string()),
                    ("sort", "recentupdate".to_string()),
                    ("page", page.to_string()),
                    ("limit", limit.to_string()),
                ])
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                bail!("Failed to list Gitea pull requests: {status} - {error_text}");
            }

            let page_pulls: Vec<GiteaPullRequest> = response.json().await?;
            let page_len = page_pulls.len();
            pulls.extend(page_pulls.into_iter().map(Into::into));

            if page_len < limit {
                break;
            }
            page += 1;
        }

        Ok(pulls)
    }

    /// Create a pull request.
    pub async fn create_pull_request(
        &self,
        params: &CreatePullRequestParams<'_>,
    ) -> Result<PullRequest> {
        #[derive(Serialize)]
        struct CreatePullRequestBody<'a> {
            title: &'a str,
            body: &'a str,
            head: &'a str,
            base: &'a str,
        }

        let url = format!(
            "{}/repos/{}/{}/pulls",
            self.base_url, params.owner, params.repo
        );
        let title = update_draft_state_in_title(params.title, params.draft);
        let body = CreatePullRequestBody {
            title: &title,
            body: params.body,
            head: params.head,
            base: params.base,
        };

        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("Failed to create Gitea pull request: {status} - {error_text}");
        }

        let pr: GiteaPullRequest = response.json().await?;
        Ok(pr.into())
    }

    /// Get a pull request.
    pub async fn get_pull_request(
        &self,
        owner: &str,
        repo: &str,
        pr_number: i64,
    ) -> Result<PullRequest> {
        let url = format!(
            "{}/repos/{}/{}/pulls/{}",
            self.base_url, owner, repo, pr_number
        );

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("Failed to get Gitea pull request: {status} - {error_text}");
        }

        let pr: GiteaPullRequest = response.json().await?;
        Ok(pr.into())
    }

    /// Update a pull request.
    pub async fn update_pull_request(
        &self,
        params: &UpdatePullRequestParams<'_>,
    ) -> Result<PullRequest> {
        #[derive(Serialize)]
        struct UpdatePullRequestBody<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            title: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            body: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            base: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            state: Option<&'a str>,
        }

        let url = format!(
            "{}/repos/{}/{}/pulls/{}",
            self.base_url, params.owner, params.repo, params.pr_number
        );
        let body = UpdatePullRequestBody {
            title: params.title,
            body: params.body,
            base: params.base,
            state: params.state,
        };

        let response = self.client.patch(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("Failed to update Gitea pull request: {status} - {error_text}");
        }

        let pr: GiteaPullRequest = response.json().await?;
        Ok(pr.into())
    }

    /// Merge a pull request.
    pub async fn merge_pull_request(&self, params: &MergePullRequestParams<'_>) -> Result<()> {
        #[derive(Serialize)]
        struct MergePullRequestBody<'a> {
            #[serde(rename = "Do", skip_serializing_if = "Option::is_none")]
            merge_method: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            merge_title_field: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            merge_message_field: Option<&'a str>,
        }

        let url = format!(
            "{}/repos/{}/{}/pulls/{}/merge",
            self.base_url, params.owner, params.repo, params.pr_number
        );
        let body = MergePullRequestBody {
            merge_method: params.merge_method,
            merge_title_field: params.commit_title,
            merge_message_field: params.commit_message,
        };

        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            bail!("Failed to merge Gitea pull request: {status} - {error_text}");
        }

        Ok(())
    }

    /// Set the draftiness of a pull request.
    pub async fn set_pull_request_draft_state(
        &self,
        params: &SetPullRequestDraftStateParams<'_>,
    ) -> Result<()> {
        if params.pr_number <= 0 {
            bail!("PR number must be greater than 0");
        }

        let pr = self
            .get_pull_request(params.owner, params.repo, params.pr_number)
            .await?;
        let next_title = update_draft_state_in_title(&pr.title, params.draft);
        if next_title == pr.title {
            return Ok(());
        }

        let update_params = UpdatePullRequestParams {
            owner: params.owner,
            repo: params.repo,
            pr_number: params.pr_number,
            title: Some(&next_title),
            body: None,
            base: None,
            state: None,
        };
        self.update_pull_request(&update_params).await?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }
}

fn is_likely_commit_sha(reference: &str) -> bool {
    let len = reference.len();
    (7..=40).contains(&len) && reference.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_unresolvable_ref_status(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::NOT_FOUND
            | reqwest::StatusCode::BAD_REQUEST
            | reqwest::StatusCode::UNPROCESSABLE_ENTITY
    )
}

/// Parameters to create a Gitea pull request.
pub struct CreatePullRequestParams<'a> {
    /// Repository owner.
    pub owner: &'a str,
    /// Repository name.
    pub repo: &'a str,
    /// Pull request title.
    pub title: &'a str,
    /// Pull request body.
    pub body: &'a str,
    /// Source branch, optionally prefixed with `owner:`.
    pub head: &'a str,
    /// Target branch.
    pub base: &'a str,
    /// Whether to mark the pull request as WIP.
    pub draft: bool,
}

/// Parameters to update a Gitea pull request.
pub struct UpdatePullRequestParams<'a> {
    /// Repository owner.
    pub owner: &'a str,
    /// Repository name.
    pub repo: &'a str,
    /// Pull request number.
    pub pr_number: i64,
    /// Optional title update.
    pub title: Option<&'a str>,
    /// Optional body update.
    pub body: Option<&'a str>,
    /// Optional target branch update.
    pub base: Option<&'a str>,
    /// Optional state update.
    pub state: Option<&'a str>,
}

/// Parameters to merge a Gitea pull request.
pub struct MergePullRequestParams<'a> {
    /// Repository owner.
    pub owner: &'a str,
    /// Repository name.
    pub repo: &'a str,
    /// Pull request number.
    pub pr_number: i64,
    /// Optional commit title.
    pub commit_title: Option<&'a str>,
    /// Optional commit message.
    pub commit_message: Option<&'a str>,
    /// Optional Gitea merge method.
    pub merge_method: Option<&'a str>,
}

/// Parameters to set a Gitea pull request draft state.
pub struct SetPullRequestDraftStateParams<'a> {
    /// Repository owner.
    pub owner: &'a str,
    /// Repository name.
    pub repo: &'a str,
    /// Pull request number.
    pub pr_number: i64,
    /// Whether the pull request should be marked WIP.
    pub draft: bool,
}

/// A user returned from the authenticated-user endpoint.
#[derive(Debug, Serialize)]
pub struct AuthenticatedUser {
    /// The Gitea username.
    pub username: String,
    /// URL to the user's avatar image, if available.
    pub avatar_url: Option<String>,
    /// The user's display name, if available.
    pub name: Option<String>,
    /// The user's email address, if available.
    pub email: Option<String>,
}

/// A Gitea user.
#[derive(Debug, Serialize)]
pub struct GiteaUser {
    /// The user ID.
    pub id: i64,
    /// The Gitea username.
    pub login: String,
    /// The user's display name, if available.
    pub name: Option<String>,
    /// The user's email address, if available.
    pub email: Option<String>,
    /// URL to the user's avatar image, if available.
    pub avatar_url: Option<String>,
    /// Whether the user is a bot.
    pub is_bot: bool,
}

/// A Gitea label.
#[derive(Debug, Serialize)]
pub struct GiteaLabel {
    /// The label name.
    pub name: String,
    /// The label description.
    pub description: Option<String>,
    /// The label color.
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GiteaRepository {
    pub permissions: Option<GiteaRepoPermissions>,
    pub fork: bool,
    pub private: bool,
    pub delete_branch_on_merge: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GiteaRepoPermissions {
    pub admin: bool,
    pub maintain: bool,
    pub push: bool,
    pub triage: bool,
    pub pull: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GiteaCommitStatus {
    pub id: i64,
    pub context: String,
    pub description: Option<String>,
    pub status: String,
    pub target_url: Option<String>,
    pub url: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub head_sha: String,
}

/// A Gitea pull request.
#[derive(Debug, Serialize)]
pub struct PullRequest {
    /// The pull request web URL.
    pub html_url: String,
    /// The pull request number.
    pub number: i64,
    /// The pull request title.
    pub title: String,
    /// The pull request body.
    pub body: Option<String>,
    /// The pull request author.
    pub author: Option<GiteaUser>,
    /// The pull request labels.
    pub labels: Vec<GiteaLabel>,
    /// Whether the pull request is marked WIP.
    pub draft: bool,
    /// The source branch.
    pub source_branch: String,
    /// The target branch.
    pub target_branch: String,
    /// The head commit SHA.
    pub sha: String,
    /// ISO 8601 creation timestamp.
    pub created_at: Option<String>,
    /// ISO 8601 update timestamp.
    pub updated_at: Option<String>,
    /// ISO 8601 merge timestamp.
    pub merged_at: Option<String>,
    /// ISO 8601 close timestamp.
    pub closed_at: Option<String>,
    /// SSH clone URL of the head repository.
    pub repository_ssh_url: Option<String>,
    /// HTTPS clone URL of the head repository.
    pub repository_https_url: Option<String>,
    /// Owner of the head repository.
    pub repo_owner: Option<String>,
    /// Requested reviewers.
    pub requested_reviewers: Vec<GiteaUser>,
    pub mergeable: Option<bool>,
    pub comments_count: i64,
}

#[derive(Debug, Deserialize)]
struct GiteaApiUser {
    id: i64,
    login: String,
    full_name: Option<String>,
    #[serde(default)]
    email: Option<String>,
    avatar_url: Option<String>,
    #[serde(default)]
    restricted: bool,
}

impl From<GiteaApiUser> for GiteaUser {
    fn from(user: GiteaApiUser) -> Self {
        GiteaUser {
            id: user.id,
            login: user.login,
            name: user.full_name,
            email: user.email,
            avatar_url: user.avatar_url,
            is_bot: user.restricted,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GiteaApiLabel {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    color: Option<String>,
}

impl From<GiteaApiLabel> for GiteaLabel {
    fn from(label: GiteaApiLabel) -> Self {
        GiteaLabel {
            name: label.name,
            description: label.description,
            color: label.color,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GiteaApiRepository {
    #[serde(default)]
    clone_url: Option<String>,
    #[serde(default)]
    ssh_url: Option<String>,
    #[serde(default)]
    owner: Option<GiteaApiUser>,
}

#[derive(Debug, Deserialize)]
struct GiteaApiPullRef {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    r#ref: Option<String>,
    #[serde(default)]
    sha: Option<String>,
    #[serde(default)]
    repo: Option<GiteaApiRepository>,
}

#[derive(Debug, Deserialize)]
struct GiteaPullRequest {
    html_url: String,
    number: i64,
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    user: Option<GiteaApiUser>,
    #[serde(default)]
    labels: Option<Vec<GiteaApiLabel>>,
    #[serde(default)]
    draft: Option<bool>,
    #[serde(default)]
    head: Option<GiteaApiPullRef>,
    #[serde(default)]
    base: Option<GiteaApiPullRef>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    merged_at: Option<String>,
    #[serde(default)]
    closed_at: Option<String>,
    #[serde(default)]
    requested_reviewers: Option<Vec<GiteaApiUser>>,
    #[serde(default)]
    mergeable: Option<bool>,
    #[serde(default)]
    comments: i64,
}

impl From<GiteaPullRequest> for PullRequest {
    fn from(pr: GiteaPullRequest) -> Self {
        let head = pr.head;
        let base = pr.base;
        let head_repo = head.as_ref().and_then(|head| head.repo.as_ref());

        PullRequest {
            html_url: pr.html_url,
            number: pr.number,
            draft: pr.draft.unwrap_or_else(|| has_draft_prefix(&pr.title)),
            title: pr.title,
            body: pr.body,
            author: pr.user.map(Into::into),
            labels: pr
                .labels
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            source_branch: ref_name(head.as_ref()),
            target_branch: ref_name(base.as_ref()),
            sha: head
                .as_ref()
                .and_then(|head| head.sha.clone())
                .unwrap_or_default(),
            created_at: pr.created_at,
            updated_at: pr.updated_at,
            merged_at: pr.merged_at,
            closed_at: pr.closed_at,
            repository_ssh_url: head_repo.and_then(|repo| repo.ssh_url.clone()),
            repository_https_url: head_repo.and_then(|repo| repo.clone_url.clone()),
            repo_owner: head_repo
                .and_then(|repo| repo.owner.as_ref())
                .map(|owner| owner.login.clone()),
            requested_reviewers: pr
                .requested_reviewers
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            mergeable: pr.mergeable,
            comments_count: pr.comments,
        }
    }
}

fn ref_name(reference: Option<&GiteaApiPullRef>) -> String {
    reference
        .and_then(|reference| reference.r#ref.as_ref().or(reference.name.as_ref()))
        .cloned()
        .unwrap_or_default()
}

/// Normalize a Gitea instance URL.
pub fn normalize_host(host: &str) -> Result<String> {
    let trimmed = host.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        bail!("Gitea host cannot be empty");
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let mut url = Url::parse(&with_scheme).context("Invalid Gitea host URL")?;
    url.set_query(None);
    url.set_fragment(None);
    url.set_username("").ok();
    url.set_password(None).ok();

    let path = url.path().trim_end_matches('/').to_string();
    let web_path = path.strip_suffix(GITEA_API_PATH).unwrap_or(&path);
    url.set_path(web_path.trim_end_matches('/'));

    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn api_base_url(host: &str) -> Result<String> {
    let normalized = normalize_host(host)?;
    Ok(format!("{normalized}{GITEA_API_PATH}"))
}

fn update_draft_state_in_title(title: &str, is_draft: bool) -> String {
    if is_draft {
        if has_draft_prefix(title) {
            title.to_owned()
        } else {
            format!("WIP: {title}")
        }
    } else {
        remove_draft_prefix(title).to_owned()
    }
}

fn has_draft_prefix(title: &str) -> bool {
    split_draft_prefix(title).is_some()
}

fn remove_draft_prefix(title: &str) -> &str {
    split_draft_prefix(title).unwrap_or(title)
}

fn split_draft_prefix(title: &str) -> Option<&str> {
    let title = title.trim_start();

    if let Some((prefix, rest)) = title.split_once(':') {
        let prefix = prefix.trim();
        if prefix.eq_ignore_ascii_case("wip") {
            return Some(rest.trim_start());
        }
    }

    if let Some(bracketed) = title.strip_prefix('[')
        && let Some((prefix, rest)) = bracketed.split_once(']')
    {
        let prefix = prefix.trim();
        if prefix.eq_ignore_ascii_case("wip") {
            return Some(rest.trim_start());
        }
    }

    None
}

pub(crate) fn resolve_account(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    storage: &but_forge_storage::Controller,
) -> Result<crate::GiteaAccountIdentifier, anyhow::Error> {
    let known_accounts = crate::token::list_known_gitea_accounts(storage)?;
    Ok(select_account(preferred_account, &known_accounts)?.to_owned())
}

fn select_account<'a>(
    preferred_account: Option<&crate::GiteaAccountIdentifier>,
    known_accounts: &'a [crate::GiteaAccountIdentifier],
) -> Result<&'a crate::GiteaAccountIdentifier, anyhow::Error> {
    let Some(default_account) = known_accounts.first() else {
        bail!(
            "No authenticated Gitea users found.\nRun 'but config forge auth' to authenticate with Gitea."
        );
    };
    if let Some(account) = preferred_account {
        if let Some(known_account) = known_accounts
            .iter()
            .find(|known_account| same_account_identity(known_account, account))
        {
            Ok(known_account)
        } else {
            bail!(
                "Preferred Gitea account '{account}' has not authenticated yet.\nRun 'but config forge auth' to authenticate, or choose another account."
            );
        }
    } else if known_accounts.len() == 1 {
        Ok(default_account)
    } else {
        bail!(
            "Multiple authenticated Gitea users found.\nChoose a Gitea account for this project before using Gitea review operations."
        );
    }
}

fn same_account_identity(
    left: &crate::GiteaAccountIdentifier,
    right: &crate::GiteaAccountIdentifier,
) -> bool {
    left.username() == right.username()
        && normalized_host_identity(left.host()) == normalized_host_identity(right.host())
}

fn normalized_host_identity(host: &str) -> String {
    normalize_host(host).unwrap_or_else(|_| host.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        is_likely_commit_sha, is_unresolvable_ref_status, normalize_host, same_account_identity,
        select_account, update_draft_state_in_title,
    };
    use crate::GiteaAccountIdentifier;

    #[test]
    fn normalizes_plain_host_to_https_root() {
        assert_eq!(
            normalize_host("gitea.example.com").unwrap(),
            "https://gitea.example.com"
        );
    }

    #[test]
    fn strips_api_suffix_from_host() {
        assert_eq!(
            normalize_host("https://gitea.example.com/api/v1").unwrap(),
            "https://gitea.example.com"
        );
    }

    #[test]
    fn preserves_subpath_instances() {
        assert_eq!(
            normalize_host("https://example.com/git/api/v1").unwrap(),
            "https://example.com/git"
        );
    }

    #[test]
    fn makes_title_wip() {
        assert_eq!(
            update_draft_state_in_title("Add API validation", true),
            "WIP: Add API validation"
        );
    }

    #[test]
    fn draft_state_noops_when_already_wip() {
        assert_eq!(
            update_draft_state_in_title("WIP: Add API validation", true),
            "WIP: Add API validation"
        );
        assert_eq!(
            update_draft_state_in_title("[wip] Add API validation", true),
            "[wip] Add API validation"
        );
    }

    #[test]
    fn removes_wip_prefix_for_ready_state() {
        assert_eq!(
            update_draft_state_in_title("WIP: Add API validation", false),
            "Add API validation"
        );
        assert_eq!(
            update_draft_state_in_title("[WIP] Add API validation", false),
            "Add API validation"
        );
    }

    #[test]
    fn preferred_account_matches_known_account() {
        let known = GiteaAccountIdentifier::selfhosted("alice", "https://gitea.example.com");
        let preferred = GiteaAccountIdentifier::selfhosted("alice", "https://gitea.example.com");

        assert!(same_account_identity(&known, &preferred));
    }

    #[test]
    fn refuses_default_account_when_multiple_gitea_accounts_are_known() {
        let accounts = vec![
            GiteaAccountIdentifier::selfhosted("alice", "https://gitea.example.com"),
            GiteaAccountIdentifier::selfhosted("alice", "https://gitea.internal"),
        ];

        let err = select_account(None, &accounts).unwrap_err().to_string();

        assert!(
            err.contains("Multiple authenticated Gitea users"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn full_and_short_hex_are_treated_as_commit_shas() {
        assert!(is_likely_commit_sha("deadbee"));
        assert!(is_likely_commit_sha(
            "0123456789abcdef0123456789abcdef01234567"
        ));
        assert!(!is_likely_commit_sha("main"));
        assert!(!is_likely_commit_sha("fix/ksef-invoice-candidates-build"));
    }

    #[test]
    fn gitea_unresolvable_ref_statuses_match_github_and_bitbucket_contract() {
        assert!(is_unresolvable_ref_status(reqwest::StatusCode::BAD_REQUEST));
        assert!(is_unresolvable_ref_status(reqwest::StatusCode::NOT_FOUND));
        assert!(is_unresolvable_ref_status(
            reqwest::StatusCode::UNPROCESSABLE_ENTITY
        ));
        assert!(!is_unresolvable_ref_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ));
    }
}

#[cfg(test)]
#[path = "client_checks_tests.rs"]
mod client_checks_tests;
