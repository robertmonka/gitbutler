//! Claude Code hook for workspace awareness and skill activation.
//!
//! Outputs workspace status as JSON plus a skill-loading nudge.
//! Intended to fire on the `Stop` hook so the agent sees actionable
//! uncommitted changes and is reminded to use the `but` skill for version
//! control.
//!
//! Design: best-effort, never fail. Always exits 0 — errors propagate
//! to a `catch_unwind` boundary so panics (e.g. broken pipe) cannot escape.
use std::io::{IsTerminal as _, Read as _, Write as _};

use bstr::ByteSlice;
use but_core::TreeStatusKind;
use serde::Serialize;

/// Main entry point for `but eval-hook`.
///
/// Uses `catch_unwind` to enforce the "never fail" contract — even panics
/// (e.g. from broken pipe on stdout/stderr) are caught and silently ignored.
pub fn execute() {
    let agent = classify_agent(read_hook_input().as_ref());

    match std::panic::catch_unwind(|| output_status(&agent)) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::debug!(?e, "eval-hook: failed to output status");
            let _ = write_empty_json_response(&agent);
        }
        Err(_) => {
            let _ = write_empty_json_response(&agent);
        }
    }
}

/// Which agent invoked the stop hook, with the per-agent state the response
/// depends on. Claude sees plain stdout; Codex and Cursor require JSON stdout.
enum StopAgent {
    Codex,
    Cursor { loop_count: u64 },
    Other,
}

fn classify_agent(input: Option<&serde_json::Value>) -> StopAgent {
    let Some(input) = input else {
        return StopAgent::Other;
    };
    if is_cursor_stop_hook_input(input) {
        StopAgent::Cursor {
            loop_count: cursor_loop_count(input),
        }
    } else if is_codex_stop_hook_input(input) {
        StopAgent::Codex
    } else {
        StopAgent::Other
    }
}

/// Workspace status as returned by the hook.
/// Contains worktree changes (from `worktree_changes_no_renames()`) and
/// branch/stack info (from `head_info()`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HookStatus {
    uncommitted_file_count: usize,
    uncommitted_files: Vec<FileChange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stacks: Vec<StackInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileChange {
    path: String,
    change_type: &'static str,
}

/// A stack with its branches and commit counts — enough for the agent
/// to understand the workspace shape without the full `but status` payload.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StackInfo {
    branches: Vec<BranchInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchInfo {
    name: String,
    commit_count: usize,
    push_status: but_workspace::ui::PushStatus,
}

fn output_status(agent: &StopAgent) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let ctx = but_ctx::Context::discover(&cwd)?;
    let repo = ctx.repo.get()?;

    // Worktree changes — skips rename tracking for faster response.
    let worktree_changes = but_core::diff::worktree_changes_no_renames(&repo)?;

    let uncommitted_files: Vec<FileChange> = worktree_changes
        .changes
        .iter()
        .map(|c| FileChange {
            path: c.path.to_str_lossy().into_owned(),
            change_type: match c.status.kind() {
                TreeStatusKind::Addition => "added",
                TreeStatusKind::Deletion => "removed",
                TreeStatusKind::Modification => "modified",
                // Unreachable with no-renames, but kept for exhaustive match.
                TreeStatusKind::Rename => "renamed",
            },
        })
        .collect();

    // Stack/branch info via head_info — graph walk only (expensive_commit_info disabled).
    // Best-effort: if this fails (corrupted refs, etc.), we still report worktree changes.
    let stacks = match collect_stacks(&ctx, &repo) {
        Ok(stacks) => stacks,
        Err(e) => {
            tracing::debug!(?e, "eval-hook: failed to collect stack info");
            Vec::new()
        }
    };

    let status = HookStatus {
        uncommitted_file_count: uncommitted_files.len(),
        uncommitted_files,
        stacks,
    };

    let json = serde_json::to_string(&status)?;
    let message = format!(
        "but status: {json}\n\
         → Skill(gitbutler): load the `but` (gitbutler) skill for version control \
         (staging, committing, pushing, branching). Do not use raw git commands."
    );
    let has_uncommitted = status.uncommitted_file_count > 0 || !status.uncommitted_files.is_empty();

    match agent {
        StopAgent::Codex => {
            // Codex Stop hook `decision: "block"` re-enters the turn and loops
            // while the workspace remains dirty, so passive status never blocks.
            write_empty_json_response(agent)?;
        }
        StopAgent::Cursor { loop_count } => {
            // Cursor has no `decision: block`; inject a follow-up message instead.
            // Only on the first stop (`loop_count == 0`) so we never loop — Cursor's own
            // `loop_limit` is a second backstop.
            if has_uncommitted && *loop_count == 0 {
                write_cursor_followup_response(&message)?;
            } else {
                write_empty_json_response(agent)?;
            }
        }
        StopAgent::Other => {
            // Output: JSON status block + human-readable nudge.
            // The JSON gives the agent structured data; the nudge ensures skill loading.
            write!(std::io::stdout(), "{message}")?;
        }
    }
    Ok(())
}

fn write_empty_json_response(agent: &StopAgent) -> anyhow::Result<()> {
    match agent {
        StopAgent::Codex | StopAgent::Cursor { .. } => {
            serde_json::to_writer(std::io::stdout(), &serde_json::json!({}))?;
        }
        StopAgent::Other => {}
    }
    Ok(())
}

fn write_cursor_followup_response(message: &str) -> anyhow::Result<()> {
    serde_json::to_writer(
        std::io::stdout(),
        &serde_json::json!({ "followup_message": message }),
    )?;
    Ok(())
}

fn read_hook_input() -> Option<serde_json::Value> {
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }

    let mut input = String::new();
    stdin.read_to_string(&mut input).ok()?;
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    serde_json::from_str(input).ok()
}

fn is_codex_stop_hook_input(input: &serde_json::Value) -> bool {
    let Some(input) = input.as_object() else {
        return false;
    };

    // Detect Codex by `turn_id`, which is unique to Codex's Stop hook payload.
    // Do NOT key on `permission_mode` (or `model`): Claude Code also sends
    // `permission_mode` on its Stop hook, so matching it misclassifies Claude as
    // Codex and emits `decision:block` — surfaced as a "Stop hook error" instead
    // of the plain skill-loading nudge the Claude path is meant to produce.
    input
        .get("hook_event_name")
        .and_then(serde_json::Value::as_str)
        == Some("Stop")
        && input.contains_key("turn_id")
}

/// True for a Cursor stop hook. Cursor uses a lowercase `hook_event_name` of
/// `"stop"` and a different payload than Codex/Claude (no `turn_id`); we key off
/// `conversation_id`, which every Cursor hook payload carries.
fn is_cursor_stop_hook_input(input: &serde_json::Value) -> bool {
    let Some(input) = input.as_object() else {
        return false;
    };

    input
        .get("hook_event_name")
        .and_then(serde_json::Value::as_str)
        == Some("stop")
        && input.contains_key("conversation_id")
}

/// Cursor's `loop_count`: how many auto-follow-ups have already happened this
/// conversation. Starts at 0; absent is treated as 0.
fn cursor_loop_count(input: &serde_json::Value) -> u64 {
    input
        .get("loop_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

/// Collect stack/branch info from `head_info()`.
fn collect_stacks(
    ctx: &but_ctx::Context,
    repo: &gix::Repository,
) -> anyhow::Result<Vec<StackInfo>> {
    let meta = ctx.meta()?;
    let info = but_workspace::head_info(
        repo,
        &meta,
        but_workspace::ref_info::Options {
            expensive_commit_info: false,
            ..Default::default()
        },
    )?;
    Ok(info
        .stacks
        .iter()
        .map(|stack| StackInfo {
            branches: stack
                .segments
                .iter()
                .filter_map(|seg| {
                    let name = seg.ref_info.as_ref()?.ref_name.shorten().to_string();
                    Some(BranchInfo {
                        name,
                        commit_count: seg.commits.len(),
                        push_status: seg.push_status,
                    })
                })
                .collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_status_serialization_shape() {
        let status = HookStatus {
            uncommitted_file_count: 2,
            uncommitted_files: vec![
                FileChange {
                    path: "foo.rs".into(),
                    change_type: "modified",
                },
                FileChange {
                    path: "bar.rs".into(),
                    change_type: "added",
                },
            ],
            stacks: vec![],
        };
        let json = serde_json::to_string(&status).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["uncommittedFileCount"], 2);
        assert_eq!(v["uncommittedFiles"].as_array().unwrap().len(), 2);
        assert_eq!(v["uncommittedFiles"][0]["changeType"], "modified");
        // Empty stacks are omitted by skip_serializing_if
        assert!(v.get("stacks").is_none());
    }
}
