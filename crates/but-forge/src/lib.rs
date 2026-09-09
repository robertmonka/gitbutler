mod forge;
pub use crate::forge::{ForgeName, ForgeRepoInfo, ForgeUser, deserialize_preferred_forge_user_opt};

mod association;
mod ci;
mod db;
pub use db::{cached_review_states, list_cached_forge_reviews};
mod forge_info;
mod merge_message;
pub use merge_message::{MergedReviewFromMessage, merged_review_from_message};
mod remote_url;
mod repo;
mod review;
pub use association::{
    ReviewAssociation, preferred_review, review_associations_by_head, review_for_head_ref,
    reviews_by_head,
};
pub use ci::{CiCheck, CiConclusion, CiOutput, CiStatus, ci_checks_for_ref_with_cache};
pub use forge_info::{
    ForgeCapabilities, ForgeInfo, ForgeUnitInfo, compare_branch_url,
    compare_branch_url_for_project, forge_info, forge_info_for_project,
};
pub use repo::{RepoInfo, RepoPermissions, get_repo_info};
pub use review::{
    CacheConfig, CreateForgeReviewParams, ForgeAccountValidity, ForgeReview, ForgeReviewComment,
    ForgeReviewFilter, ForgeReviewLabel, ForgeReviewReaction, ForgeReviewReactionCount,
    ForgeReviewSubmission, ForgeReviewSubmissionState, ForgeReviewTargetUpdate, ForgeReviewThread,
    ForgeReviewThreadComment, ForgeReviewThreadSide, ForgeReviewTimelineEvent,
    ForgeReviewTimelineEventKind, ForgeReviewUpdate, ForgeReviewUser, GitHubStackingMode,
    PublishReviewOutcome, ReviewMergeMethod, ReviewMergeStatus, ReviewStackingDescription,
    ReviewState, ReviewSyncOutcome, ReviewTemplateFunctions, ReviewUpdatePayload,
    add_comment_reaction, add_review_labels, add_review_reaction, add_submission_reaction,
    available_review_templates, cache_review, check_forge_account_is_valid,
    compute_review_target_updates, create_forge_review, create_review_comment,
    create_review_thread_reply, delete_review_comment, get_forge_review, get_review_base_repo_url,
    get_review_merge_status, get_review_template_functions, list_comment_reactions,
    list_forge_reviews_for_branch, list_forge_reviews_with_cache, list_repo_labels,
    list_review_comments, list_review_reactions, list_review_submissions, list_review_threads,
    list_review_timeline_events, list_reviewer_candidates, merge_review,
    prepare_review_target_updates, remove_comment_reaction, remove_review_label,
    remove_review_reaction, remove_submission_reaction, request_review, restore_native_stacks,
    set_review_auto_merge_state, set_review_draftiness, sync_reviews, update_review,
    update_review_comment, withdraw_review_request,
};

fn determine_forge_from_host(host: &str) -> Option<ForgeName> {
    if host.contains("github.com") || host.starts_with("github.") {
        Some(ForgeName::GitHub)
    } else if host.contains("gitlab.com") || host.starts_with("gitlab.") {
        Some(ForgeName::GitLab)
    } else if host.contains("gitea") {
        Some(ForgeName::Gitea)
    } else if host.contains("bitbucket.org") {
        Some(ForgeName::Bitbucket)
    } else if host.contains("azure.com") {
        Some(ForgeName::Azure)
    } else {
        None
    }
}

/// Derive the forge repository information from a remote URL.
///
/// If the forge type can't be determined by simply looking for keywords in the
/// repository URL, look through all known accounts and try to match their custom
/// host strings to the repository URL host. Gitea also consults known accounts
/// so self-hosted instances mounted under a base path can strip that path before
/// deriving the owner and repository.
pub fn derive_forge_repo_info(url: &str) -> Option<ForgeRepoInfo> {
    let remote = remote_url::RemoteUrl::parse(url)?;
    let host_forge = determine_forge_from_host(&remote.host);
    let accounts = if matches!(host_forge, Some(ForgeName::Gitea)) || host_forge.is_none() {
        get_all_forge_accounts().unwrap_or_default()
    } else {
        Vec::new()
    };

    derive_forge_repo_info_from_remote(&remote, host_forge, &accounts)
}

pub fn derive_forge_repo_info_with_forge(url: &str, forge: ForgeName) -> Option<ForgeRepoInfo> {
    let remote = remote_url::RemoteUrl::parse(url)?;
    let (owner, repo) = remote.repository_parts(&forge)?;

    Some(ForgeRepoInfo {
        forge,
        owner,
        repo,
        protocol: remote.protocol,
    })
}

/// Derive forge repository information using automatic detection and project hints.
///
/// Detection order: explicit [`ForgeName`] override, then hostname keywords,
/// then the preferred account's forge when the host is ambiguous, then known
/// accounts whose custom host matches the remote. Preferred account must not
/// override a remote that already identifies as another forge.
pub fn derive_forge_repo_info_for_project(
    url: &str,
    forge_override: Option<ForgeName>,
    preferred_user: Option<&ForgeUser>,
) -> Option<ForgeRepoInfo> {
    let remote = remote_url::RemoteUrl::parse(url)?;
    let repository_host = remote.host.as_str();
    let host_from_name = determine_forge_from_host(repository_host);
    let configured_forge = forge_override.or_else(|| {
        // Preferred account only fills hosts that do not already name a forge.
        // Otherwise Open links for github.com remotes jump to an unrelated Gitea.
        if host_from_name.is_some() {
            None
        } else {
            preferred_user.map(ForgeUser::forge_name)
        }
    });
    let host_forge = configured_forge
        .clone()
        .or(host_from_name)
        .or_else(|| {
            preferred_user.and_then(|user| match user {
                ForgeUser::Gitea(account)
                    if gitea_account_matches_repository(repository_host, account) =>
                {
                    Some(ForgeName::Gitea)
                }
                _ => None,
            })
        });
    let mut accounts = if matches!(host_forge, Some(ForgeName::Gitea)) || host_forge.is_none() {
        get_all_forge_accounts().unwrap_or_default()
    } else {
        Vec::new()
    };

    prioritize_accounts_for_repository_host(&mut accounts, repository_host, preferred_user);

    derive_forge_repo_info_from_remote(&remote, host_forge, &accounts)
        .or_else(|| configured_forge.and_then(|forge| derive_forge_repo_info_with_forge(url, forge)))
}

fn prioritize_accounts_for_repository_host(
    accounts: &mut Vec<ForgeUser>,
    repository_host: &str,
    preferred_user: Option<&ForgeUser>,
) {
    accounts.sort_by_key(|account| account_priority(account, repository_host));

    let Some(preferred) = preferred_user else {
        return;
    };
    if account_priority(preferred, repository_host) != 0 {
        return;
    }
    if let Some(index) = accounts.iter().position(|account| account == preferred) {
        let preferred = accounts.remove(index);
        accounts.insert(0, preferred);
    } else {
        accounts.insert(0, preferred.clone());
    }
}

fn account_priority(account: &ForgeUser, repository_host: &str) -> u8 {
    let matches = match account {
        ForgeUser::GitHub(github) => github
            .custom_host()
            .is_some_and(|host| custom_host_matches_repository_host(repository_host, &host)),
        ForgeUser::GitLab(gitlab) => gitlab
            .custom_host()
            .is_some_and(|host| custom_host_matches_repository_host(repository_host, &host)),
        ForgeUser::Bitbucket(bitbucket) => bitbucket
            .custom_host()
            .is_some_and(|host| custom_host_matches_repository_host(repository_host, &host)),
        ForgeUser::Gitea(gitea) => gitea_account_matches_repository(repository_host, gitea),
    };
    if matches { 0 } else { 1 }
}

#[cfg(test)]
fn derive_forge_repo_info_with_accounts(
    url: &str,
    accounts: &[ForgeUser],
) -> Option<ForgeRepoInfo> {
    let remote = remote_url::RemoteUrl::parse(url)?;
    let host_forge = determine_forge_from_host(&remote.host);
    derive_forge_repo_info_from_remote(&remote, host_forge, accounts)
}

fn derive_forge_repo_info_from_remote(
    remote: &remote_url::RemoteUrl,
    host_forge: Option<ForgeName>,
    accounts: &[ForgeUser],
) -> Option<ForgeRepoInfo> {
    let account_match = match_host_to_accounts_custom_host_with_value(&remote.host, accounts);
    let forge = host_forge.or_else(|| {
        account_match
            .as_ref()
            .map(|(account_forge, _)| account_forge.clone())
    })?;

    let (owner, repo) = account_match
        .as_ref()
        .filter(|(account_forge, _)| account_forge == &forge)
        .and_then(|(_, custom_host)| owner_repo_after_custom_host_path(&remote.path, custom_host))
        .or_else(|| remote.repository_parts(&forge))?;

    Some(ForgeRepoInfo {
        forge,
        owner,
        repo,
        protocol: remote.protocol.clone(),
    })
}

/// Look for the best matching account by comparing the repository host to the
/// account custom host string.
#[cfg(test)]
fn match_host_to_accounts_custom_host(host: &str, accounts: &[ForgeUser]) -> Option<ForgeName> {
    match_host_to_accounts_custom_host_with_value(host, accounts).map(|(forge, _)| forge)
}

fn match_host_to_accounts_custom_host_with_value(
    host: &str,
    accounts: &[ForgeUser],
) -> Option<(ForgeName, String)> {
    for account in accounts {
        match account {
            ForgeUser::Gitea(gitea_account) => {
                if let Some(custom_host) =
                    gitea_account_custom_host_for_repository(host, gitea_account)
                {
                    return Some((ForgeName::Gitea, custom_host));
                }
            }
            _ => {
                if let Some(custom_host) = account.custom_host()
                    && custom_host_matches_repository_host(host, &custom_host)
                {
                    return Some((account.forge_name(), custom_host));
                }
            }
        }
    }

    None
}


fn owner_repo_after_custom_host_path(
    repository_path: &str,
    custom_host: &str,
) -> Option<(String, String)> {
    let custom_path = custom_host_path(custom_host)?;
    let repository_path = repository_path.trim_start_matches('/');
    let remainder = repository_path.strip_prefix(&custom_path)?;
    let remainder = remainder.strip_prefix('/')?;
    owner_repo_from_path(remainder)
}

fn owner_repo_from_path(path: &str) -> Option<(String, String)> {
    let path = path.trim_start_matches('/').trim_end_matches('/');
    let mut parts = path.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim().trim_end_matches(".git");

    if owner.is_empty() || repo.is_empty() {
        return None;
    }

    Some((owner.to_string(), repo.to_string()))
}

fn custom_host_path(custom_host: &str) -> Option<String> {
    let without_scheme = custom_host
        .split_once("://")
        .map_or(custom_host, |(_, rest)| rest);
    let path = without_scheme.split_once('/')?.1;
    let path = path
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_matches('/');

    (!path.is_empty()).then(|| path.to_string())
}

/// Compare a repository host to an account custom-host string.
///
/// Motivation:
/// account custom hosts may be stored as full API endpoints (for example
/// `https://api.repository.com/v1/api`), while repository remotes usually
/// provide only the repository host (`repository.com`).
///
/// Behavior:
/// - both inputs are normalized (scheme, path/query/fragment, user-info, and
///   numeric port are removed; casing is ignored)
/// - exact host matches return `true`
/// - subdomain custom-hosts match their root repository host
///   (`api.repository.com` matches `repository.com`)
/// - partial suffixes do not match (`api.notrepository.com` does not match
///   `repository.com`)
fn gitea_account_matches_repository(
    repository_host: &str,
    account: &but_gitea::GiteaAccountIdentifier,
) -> bool {
    gitea_account_custom_host_for_repository(repository_host, account).is_some()
}

pub(crate) fn gitea_web_host_for_repository(
    repository_host: &str,
    account: &but_gitea::GiteaAccountIdentifier,
) -> Option<String> {
    gitea_account_matches_repository(repository_host, account).then(|| {
        account
            .view_host()
            .unwrap_or_else(|| account.host())
            .to_string()
    })
}

fn gitea_account_custom_host_for_repository(
    repository_host: &str,
    account: &but_gitea::GiteaAccountIdentifier,
) -> Option<String> {
    let mut hosts = vec![(account.host(), account.host().to_string())];
    if let Some(view_host) = account.view_host() {
        hosts.push((view_host, view_host.to_string()));
    }
    if let Some((_, custom_host)) = hosts
        .iter()
        .find(|(host, _)| custom_host_matches_repository_host(repository_host, host))
    {
        return Some(custom_host.clone());
    }

    if !account.hosts_contain_gitea_keyword() {
        return None;
    }

    let repository = normalize_host_for_comparison(repository_host);
    hosts.into_iter().find_map(|(host, custom_host)| {
        let account_host = normalize_host_for_comparison(host);
        hosts_share_parent_domain(&repository, &account_host).then_some(custom_host)
    })
}

/// Split-host setups: git on `ssh.example.com`, UI on `gitea.example.com`.
fn hosts_share_parent_domain(repository_host: &str, account_host: &str) -> bool {
    if repository_host == account_host {
        return true;
    }
    match (
        parent_domain_suffix(repository_host),
        parent_domain_suffix(account_host),
    ) {
        (Some(repository_parent), Some(account_parent))
            if repository_parent == account_parent && repository_parent.contains('.') =>
        {
            true
        }
        _ => false,
    }
}

fn parent_domain_suffix(host: &str) -> Option<String> {
    let labels: Vec<&str> = host.split('.').filter(|label| !label.is_empty()).collect();
    if labels.len() < 2 {
        return None;
    }
    Some(labels[labels.len() - 2..].join("."))
}

fn custom_host_matches_repository_host(repository_host: &str, account_custom_host: &str) -> bool {
    let normalized_repository_host = normalize_host_for_comparison(repository_host);
    let normalized_account_host = normalize_host_for_comparison(account_custom_host);

    if normalized_repository_host.is_empty() || normalized_account_host.is_empty() {
        return false;
    }

    normalized_account_host == normalized_repository_host
        || normalized_account_host.ends_with(&format!(".{normalized_repository_host}"))
}

pub(crate) fn normalize_host_for_comparison(value: &str) -> String {
    let without_scheme = value.split_once("://").map_or(value, |(_, rest)| rest);
    let without_path = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let without_user_info = without_path
        .rsplit_once('@')
        .map_or(without_path, |(_, host)| host);

    let without_port = match without_user_info.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => without_user_info,
    };

    without_port
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

/// The login this repository's forge calls authenticate as, mirroring how
/// the per-forge clients resolve their account: the preferred account when
/// it is still in storage, otherwise (with no preference set) the first
/// known account of the repository's forge. `None` when no account is
/// configured — or when the preferred account is gone from storage, where
/// the clients refuse to authenticate rather than fall back.
pub fn current_forge_login(
    preferred_forge_user: &Option<ForgeUser>,
    forge_repo_info: &ForgeRepoInfo,
    storage: &but_forge_storage::Controller,
) -> anyhow::Result<Option<String>> {
    match forge_repo_info.forge {
        ForgeName::GitHub => {
            let accounts = but_github::list_known_github_accounts(storage)?;
            let preferred = preferred_forge_user.as_ref().and_then(|user| user.github());
            Ok(match preferred {
                Some(preferred) => accounts.iter().find(|account| *account == preferred),
                None => accounts.first(),
            }
            .map(|account| account.username().to_string()))
        }
        ForgeName::GitLab => {
            let accounts = but_gitlab::list_known_gitlab_accounts(storage)?;
            let preferred = preferred_forge_user.as_ref().and_then(|user| user.gitlab());
            Ok(match preferred {
                Some(preferred) => accounts.iter().find(|account| *account == preferred),
                None => accounts.first(),
            }
            .map(|account| account.username().to_string()))
        }
        ForgeName::Gitea => {
            let accounts = but_gitea::list_known_gitea_accounts(storage)?;
            let preferred = preferred_forge_user.as_ref().and_then(|user| user.gitea());
            Ok(match preferred {
                Some(preferred) => accounts.iter().find(|account| *account == preferred),
                None => accounts.first(),
            }
            .map(|account| account.username().to_string()))
        }
        _ => Ok(None),
    }
}

/// Get all known forge accounts
pub fn get_all_forge_accounts() -> anyhow::Result<Vec<ForgeUser>> {
    let storage = but_forge_storage::Controller::from_path(but_path::app_data_dir()?);
    let gh_accounts = but_github::list_known_github_accounts(&storage)?;
    let gl_accounts = but_gitlab::list_known_gitlab_accounts(&storage)?;
    let gitea_accounts = but_gitea::list_known_gitea_accounts(&storage)?;

    let mut forge_users = vec![];
    for gh_account in gh_accounts {
        forge_users.push(ForgeUser::GitHub(gh_account));
    }

    for gl_account in gl_accounts {
        forge_users.push(ForgeUser::GitLab(gl_account));
    }

    for gitea_account in gitea_accounts {
        forge_users.push(ForgeUser::Gitea(gitea_account));
    }

    Ok(forge_users)
}

#[cfg(test)]
mod tests {
    use super::{
        ForgeName, ForgeRepoInfo, ForgeUser, current_forge_login, derive_forge_repo_info,
        derive_forge_repo_info_with_accounts, match_host_to_accounts_custom_host,
        normalize_host_for_comparison,
    };

    #[test]
    fn derives_supported_forge_repository_urls() {
        let cases = [
            (
                "git@github.com:owner/example.github.io.git",
                ForgeName::GitHub,
                "owner",
                "example.github.io",
                "ssh",
            ),
            (
                "https://github.com/owner/example.github.io.git",
                ForgeName::GitHub,
                "owner",
                "example.github.io",
                "https",
            ),
            (
                "https://github.com/owner/repository.git.git",
                ForgeName::GitHub,
                "owner",
                "repository.git",
                "https",
            ),
            (
                "https://github.com/owner/repo/",
                ForgeName::GitHub,
                "owner",
                "repo",
                "https",
            ),
            (
                "https://gitlab.com/group/subgroup/repo.git",
                ForgeName::GitLab,
                "group/subgroup",
                "repo",
                "https",
            ),
            (
                "git@bitbucket.org:owner/repo.git",
                ForgeName::Bitbucket,
                "owner",
                "repo",
                "ssh",
            ),
            (
                "https://dev.azure.com/org/project/_git/repo",
                ForgeName::Azure,
                "org/project",
                "repo",
                "https",
            ),
            (
                "git@ssh.dev.azure.com:v3/org/project/repo.git",
                ForgeName::Azure,
                "org/project",
                "repo",
                "ssh",
            ),
        ];

        for (url, forge, owner, repo, protocol) in cases {
            let actual = derive_forge_repo_info(url).unwrap();
            assert_eq!(actual.forge, forge, "forge should be derived from {url}");
            assert_eq!(actual.owner, owner, "owner should be derived from {url}");
            assert_eq!(actual.repo, repo, "repository should be derived from {url}");
            assert_eq!(
                actual.protocol, protocol,
                "protocol should be derived from {url}"
            );
        }
    }

    #[test]
    fn azure_encoded_path_characters_are_not_url_delimiters() {
        for (url, expected_repo) in [
            (
                "https://dev.azure.com/org/project/_git/repo%23one",
                "repo#one",
            ),
            (
                "https://dev.azure.com/org/project/_git/repo%3Fone",
                "repo?one",
            ),
            (
                "https://dev.azure.com/org/project/_git/repo%23one?query=value#fragment",
                "repo#one",
            ),
            (
                "https://dev.azure.com/org/project/_git/repo%2523one",
                "repo%23one",
            ),
            ("git@ssh.dev.azure.com:v3/org/project/repo#one", "repo#one"),
            ("git@ssh.dev.azure.com:v3/org/project/repo?one", "repo?one"),
            (
                "ssh://git@ssh.dev.azure.com/v3/org/project/repo%23one",
                "repo#one",
            ),
        ] {
            let actual = derive_forge_repo_info(url).unwrap();
            assert_eq!(actual.owner, "org/project", "owner should survive in {url}");
            assert_eq!(
                actual.repo, expected_repo,
                "encoded path characters are not query or fragment delimiters in {url}"
            );
        }
    }

    #[test]
    fn azure_https_organization_can_be_named_v3() {
        let actual = derive_forge_repo_info("https://dev.azure.com/v3/project/_git/repo").unwrap();
        assert_eq!(actual.forge, ForgeName::Azure, "host identifies Azure");
        assert_eq!(
            actual.owner, "v3/project",
            "v3 is an HTTPS organization, not an SSH path marker"
        );
        assert_eq!(actual.repo, "repo", "repository name should survive");
    }

    #[test]
    fn rejects_non_forge_and_malformed_repository_urls() {
        for url in [
            "../local-repo",
            "file:///tmp/repo",
            "https://github.com/owner",
        ] {
            assert!(
                derive_forge_repo_info(url).is_none(),
                "{url} does not identify a hosted forge repository"
            );
        }
    }

    fn github_repo_info() -> ForgeRepoInfo {
        ForgeRepoInfo {
            forge: ForgeName::GitHub,
            owner: "gitbutlerapp".into(),
            repo: "gitbutler".into(),
            protocol: "https".into(),
        }
    }

    fn gitlab_repo_info() -> ForgeRepoInfo {
        ForgeRepoInfo {
            forge: ForgeName::GitLab,
            owner: "gitbutlerapp".into(),
            repo: "gitbutler".into(),
            protocol: "https".into(),
        }
    }

    fn github_storage(usernames: &[&str]) -> (but_forge_storage::Controller, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let storage = but_forge_storage::Controller::from_path(tmp.path());
        for username in usernames {
            storage
                .add_github_account(&but_forge_storage::settings::GitHubAccount::OAuth {
                    username: (*username).into(),
                    access_token_key: format!("github_oauth_{username}"),
                })
                .unwrap();
        }
        (storage, tmp)
    }

    fn gitlab_storage(usernames: &[&str]) -> (but_forge_storage::Controller, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let storage = but_forge_storage::Controller::from_path(tmp.path());
        for username in usernames {
            storage
                .add_gitlab_account(&but_forge_storage::settings::GitLabAccount::Pat {
                    username: (*username).into(),
                    access_token_key: format!("gitlab_pat_{username}"),
                })
                .unwrap();
        }
        (storage, tmp)
    }

    #[test]
    fn current_login_uses_the_first_account_without_a_preference() {
        let (github, _gh_tmp) = github_storage(&["alice"]);
        let (gitlab, _gl_tmp) = gitlab_storage(&["alice"]);

        assert_eq!(
            current_forge_login(&None, &github_repo_info(), &github).unwrap(),
            Some("alice".into()),
            "the GitHub client defaults to the first stored account"
        );
        assert_eq!(
            current_forge_login(&None, &gitlab_repo_info(), &gitlab).unwrap(),
            Some("alice".into()),
            "the GitLab client defaults to the first stored account"
        );
    }

    #[test]
    fn current_login_uses_the_configured_preferred_account() {
        let (github, _gh_tmp) = github_storage(&["alice", "bob"]);
        let (gitlab, _gl_tmp) = gitlab_storage(&["alice", "bob"]);

        assert_eq!(
            current_forge_login(
                &Some(ForgeUser::GitHub(
                    but_github::GithubAccountIdentifier::oauth("bob"),
                )),
                &github_repo_info(),
                &github,
            )
            .unwrap(),
            Some("bob".into()),
            "a stored preferred account is the one the GitHub client authenticates as"
        );
        assert_eq!(
            current_forge_login(
                &Some(ForgeUser::GitLab(but_gitlab::GitlabAccountIdentifier::pat(
                    "bob"
                ),)),
                &gitlab_repo_info(),
                &gitlab,
            )
            .unwrap(),
            Some("bob".into()),
            "a stored preferred account is the one the GitLab client authenticates as"
        );
    }

    #[test]
    fn current_login_rejects_a_missing_preferred_account() {
        let (github, _gh_tmp) = github_storage(&["alice"]);
        let (gitlab, _gl_tmp) = gitlab_storage(&["alice"]);

        assert_eq!(
            current_forge_login(
                &Some(ForgeUser::GitHub(
                    but_github::GithubAccountIdentifier::oauth("forgotten"),
                )),
                &github_repo_info(),
                &github,
            )
            .unwrap(),
            None,
            "the GitHub client refuses a preferred account gone from storage, so no login is reported"
        );
        assert_eq!(
            current_forge_login(
                &Some(ForgeUser::GitLab(but_gitlab::GitlabAccountIdentifier::pat(
                    "forgotten"
                ),)),
                &gitlab_repo_info(),
                &gitlab,
            )
            .unwrap(),
            None,
            "the GitLab client refuses a preferred account gone from storage, so no login is reported"
        );
    }

    #[test]
    fn matches_github_enterprise_custom_host() {
        let accounts = vec![ForgeUser::GitHub(
            but_github::GithubAccountIdentifier::enterprise("alice", "gh.example.com"),
        )];

        assert_eq!(
            match_host_to_accounts_custom_host("gh.example.com", &accounts),
            Some(ForgeName::GitHub)
        );
    }

    #[test]
    fn matches_gitlab_self_hosted_custom_host() {
        let accounts = vec![ForgeUser::GitLab(
            but_gitlab::GitlabAccountIdentifier::selfhosted("bob", "gl.example.com"),
        )];

        assert_eq!(
            match_host_to_accounts_custom_host("gl.example.com", &accounts),
            Some(ForgeName::GitLab)
        );
    }

    #[test]
    fn matches_gitea_self_hosted_custom_host() {
        let accounts = vec![ForgeUser::Gitea(
            but_gitea::GiteaAccountIdentifier::selfhosted("bob", "https://gitea.example.com"),
        )];

        assert_eq!(
            match_host_to_accounts_custom_host("gitea.example.com", &accounts),
            Some(ForgeName::Gitea)
        );
    }

    #[test]
    fn matches_gitea_when_api_host_matches_remote_and_only_view_url_contains_gitea() {
        let account = but_gitea::GiteaAccountIdentifier::selfhosted_with_view_host(
            "alice",
            "https://git.example.com",
            Some("https://gitea.example.com".to_string()),
        );
        assert!(!account.host().contains("gitea"));
        assert!(account.hosts_contain_gitea_keyword());
        let accounts = vec![ForgeUser::Gitea(account)];

        let info =
            derive_forge_repo_info_with_accounts("https://git.example.com/web/repo.git", &accounts)
                .unwrap();

        assert_eq!(info.forge, ForgeName::Gitea);
    }

    #[test]
    fn derives_gitea_repo_info_after_configured_instance_subpath() {
        let accounts = vec![ForgeUser::Gitea(
            but_gitea::GiteaAccountIdentifier::selfhosted("alice", "https://example.com/git"),
        )];

        let info =
            derive_forge_repo_info_with_accounts("https://example.com/git/org/repo.git", &accounts)
                .expect("repo info");

        assert_eq!(info.forge, ForgeName::Gitea);
        assert_eq!(info.owner, "org");
        assert_eq!(info.repo, "repo");
    }

    #[test]
    fn matches_gitea_when_git_and_web_hosts_share_parent_domain() {
        let account = but_gitea::GiteaAccountIdentifier::selfhosted_with_view_host(
            "alice",
            "https://gitea.example.com",
            Some("https://gitea.example.com".to_string()),
        );
        let accounts = vec![ForgeUser::Gitea(account)];

        let info = derive_forge_repo_info_with_accounts(
            "ssh://git@ssh.example.com:4444/web/repo.git",
            &accounts,
        )
        .expect("repo info");

        assert_eq!(info.forge, ForgeName::Gitea);
        assert_eq!(info.owner, "web");
        assert_eq!(info.repo, "repo");
    }

    #[test]
    fn derives_gitea_repo_info_from_explicit_forge_for_separate_ssh_host() {
        let info = super::derive_forge_repo_info_with_forge(
            "ssh://git@ssh.example.com:4444/web/repo.git",
            ForgeName::Gitea,
        )
        .expect("repo info");

        assert_eq!(info.forge, ForgeName::Gitea);
        assert_eq!(info.owner, "web");
        assert_eq!(info.repo, "repo");
    }

    #[test]
    fn preferred_gitea_user_does_not_override_github_host_detection() {
        let preferred = ForgeUser::Gitea(but_gitea::GiteaAccountIdentifier::selfhosted(
            "robert.monka",
            "https://gitea.hostarm.com",
        ));
        let info = super::derive_forge_repo_info_for_project(
            "git@github.com:gitbutlerapp/gitbutler.git",
            None,
            Some(&preferred),
        )
        .expect("repo info");

        assert_eq!(info.forge, ForgeName::GitHub);
        assert_eq!(info.owner, "gitbutlerapp");
        assert_eq!(info.repo, "gitbutler");
    }

    #[test]
    fn does_not_match_accounts_without_custom_host() {
        let accounts = vec![
            ForgeUser::GitHub(but_github::GithubAccountIdentifier::oauth("alice")),
            ForgeUser::GitHub(but_github::GithubAccountIdentifier::pat("charlie")),
            ForgeUser::GitLab(but_gitlab::GitlabAccountIdentifier::pat("bob")),
        ];

        assert_eq!(
            match_host_to_accounts_custom_host("gh.example.com", &accounts),
            None
        );
    }

    #[test]
    fn returns_none_when_custom_hosts_do_not_match() {
        let accounts = vec![
            ForgeUser::GitHub(but_github::GithubAccountIdentifier::enterprise(
                "alice",
                "gh.example.com",
            )),
            ForgeUser::GitLab(but_gitlab::GitlabAccountIdentifier::selfhosted(
                "bob",
                "gl.example.com",
            )),
        ];

        assert_eq!(
            match_host_to_accounts_custom_host("no-match.example.com", &accounts),
            None
        );
    }

    #[test]
    fn matches_repository_host_against_custom_host_with_subdomain_and_path() {
        let accounts = vec![ForgeUser::GitLab(
            but_gitlab::GitlabAccountIdentifier::selfhosted(
                "bob",
                "https://api.repository.com/v1/api",
            ),
        )];

        assert_eq!(
            match_host_to_accounts_custom_host("repository.com", &accounts),
            Some(ForgeName::GitLab)
        );
    }

    #[test]
    fn matches_repository_host_against_custom_host_with_scheme_port_and_path() {
        let accounts = vec![ForgeUser::GitHub(
            but_github::GithubAccountIdentifier::enterprise(
                "alice",
                "https://api.repository.com:8443/v1/api",
            ),
        )];

        assert_eq!(
            match_host_to_accounts_custom_host("repository.com", &accounts),
            Some(ForgeName::GitHub)
        );
    }

    #[test]
    fn does_not_match_partial_domain_suffixes() {
        let accounts = vec![ForgeUser::GitLab(
            but_gitlab::GitlabAccountIdentifier::selfhosted("bob", "api.notrepository.com/v1"),
        )];

        assert_eq!(
            match_host_to_accounts_custom_host("repository.com", &accounts),
            None
        );
    }

    #[test]
    fn matches_repository_host_case_insensitively_against_custom_host() {
        let accounts = vec![ForgeUser::GitLab(
            but_gitlab::GitlabAccountIdentifier::selfhosted(
                "bob",
                "HTTPS://API.REPOSITORY.COM/v1/api",
            ),
        )];

        assert_eq!(
            match_host_to_accounts_custom_host("Repository.COM", &accounts),
            Some(ForgeName::GitLab)
        );
    }

    #[test]
    fn normalize_host_for_comparison_strips_url_parts_and_normalizes_case() {
        assert_eq!(
            normalize_host_for_comparison("HTTPS://user@API.Repository.com:8443/v1/api?x=1#frag"),
            "api.repository.com"
        );
    }

    #[test]
    fn normalize_host_for_comparison_trims_whitespace_and_trailing_dot() {
        assert_eq!(
            normalize_host_for_comparison("  repository.com.  "),
            "repository.com"
        );
    }
}
