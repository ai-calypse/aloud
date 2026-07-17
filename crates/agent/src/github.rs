//! The GitHub connector — the first real connector (Phase 1).
//!
//! It shells out to the `gh` CLI rather than speaking the REST API directly.
//! That is the whole point: `gh` already owns the auth, so there is no OAuth
//! app to register, no token to store, and no redirect to handle. If the user
//! can run `gh issue create` in a terminal, agent mode can too.
//!
//! The upgrade path, if `gh` proves too limiting, is a REST connector with its
//! own `TokenStore`. Until then this is the lazy correct choice.

use crate::connector::{Connector, ExecutionResult};
use crate::schema::ParsedAction;
use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::process::Command;

pub struct GitHubConnector;

impl GitHubConnector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GitHubConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Connector for GitHubConnector {
    fn app(&self) -> &'static str {
        "github"
    }

    fn supported_actions(&self) -> Vec<&'static str> {
        vec!["github.create_issue", "github.list_issues"]
    }

    /// Ready when `gh` is installed and authenticated. `gh auth status` exits
    /// non-zero when logged out, which is exactly the check we want.
    async fn is_ready(&self) -> bool {
        Command::new("gh")
            .args(["auth", "status"])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn execute(&self, action: &ParsedAction) -> Result<ExecutionResult> {
        match action.action.as_str() {
            "github.create_issue" => self.create_issue(action).await,
            "github.list_issues" => self.list_issues(action).await,
            other => Ok(ExecutionResult::failed(
                format!("GitHub connector can't do {other}"),
                false,
            )),
        }
    }
}

impl GitHubConnector {
    async fn create_issue(&self, action: &ParsedAction) -> Result<ExecutionResult> {
        let repo = entity(action, "repo")?;
        let title = entity(action, "title")?;
        // Body is optional; an empty --body is valid and keeps gh from opening
        // an editor, which would hang a headless invocation forever.
        let body = action
            .entities
            .get("body")
            .map(String::as_str)
            .unwrap_or("");

        let output = Command::new("gh")
            .args([
                "issue", "create", "--repo", repo, "--title", title, "--body", body,
            ])
            .output()
            .await
            .context("running gh issue create")?;

        Ok(gh_result(
            &output,
            // gh prints the new issue's URL on success.
            |url| format!("Opened issue: {}", url.trim()),
        ))
    }

    async fn list_issues(&self, action: &ParsedAction) -> Result<ExecutionResult> {
        let repo = entity(action, "repo")?;
        let output = Command::new("gh")
            .args(["issue", "list", "--repo", repo, "--limit", "10"])
            .output()
            .await
            .context("running gh issue list")?;

        Ok(gh_result(&output, |list| {
            let list = list.trim();
            if list.is_empty() {
                "No open issues.".to_string()
            } else {
                let count = list.lines().count();
                format!(
                    "{count} open issue{}:\n{list}",
                    if count == 1 { "" } else { "s" }
                )
            }
        }))
    }
}

/// A required entity, or an error naming what is missing. The dispatcher only
/// runs actions whose required entities are present, so this is belt-and-braces.
fn entity<'a>(action: &'a ParsedAction, key: &str) -> Result<&'a str> {
    action
        .entities
        .get(key)
        .map(String::as_str)
        .filter(|v| !v.trim().is_empty())
        .with_context(|| format!("missing required field: {key}"))
}

/// Maps a finished `gh` invocation to an [`ExecutionResult`].
///
/// A CLI failure is not retryable: `gh` already retries transient network
/// errors itself, so a non-zero exit here is a real problem (bad repo, no
/// permission, offline) that a blind retry would not fix.
fn gh_result(
    output: &std::process::Output,
    on_success: impl FnOnce(&str) -> String,
) -> ExecutionResult {
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        ExecutionResult::ok(on_success(&stdout))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        ExecutionResult::failed(clean_gh_error(&stderr), false)
    }
}

/// `gh` errors are prefixed and multi-line; take the first meaningful line so
/// the spoken/shown message stays short.
fn clean_gh_error(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.trim_start_matches("error:").trim().to_string())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "GitHub command failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::process::{ExitStatus, Output};

    /// Fabricates an `ExitStatus` with the given code, cross-platform.
    ///
    /// `ExitStatusExt::from_raw` differs by OS: Unix wants the raw wait status
    /// (code in the high byte), Windows wants the exit code directly. The tests
    /// only care about success vs. failure, so map `code` to each convention.
    fn exit_status(code: i32) -> ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            ExitStatus::from_raw(code << 8)
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            ExitStatus::from_raw(code as u32)
        }
    }

    fn action_with(entities: &[(&str, &str)]) -> ParsedAction {
        ParsedAction {
            action: "github.create_issue".into(),
            app: "github".into(),
            entities: entities
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            confidence: 1.0,
            requires_confirmation: true,
        }
    }

    fn output(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status: exit_status(code),
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    #[test]
    fn missing_required_entity_is_an_error() {
        let action = action_with(&[("title", "Bug")]); // no repo
        assert!(entity(&action, "repo").is_err());
    }

    #[test]
    fn blank_entity_counts_as_missing() {
        let action = action_with(&[("repo", "  ")]);
        assert!(entity(&action, "repo").is_err());
    }

    #[test]
    fn success_maps_stdout_via_the_formatter() {
        let out = output(0, "https://github.com/o/r/issues/7\n", "");
        let result = gh_result(&out, |url| format!("Opened issue: {}", url.trim()));
        assert!(result.success);
        assert_eq!(
            result.message,
            "Opened issue: https://github.com/o/r/issues/7"
        );
    }

    #[test]
    fn failure_is_not_retryable_and_message_is_cleaned() {
        // gh already retries transient network errors, so its failures are terminal.
        let out = output(
            1,
            "",
            "error: could not resolve to a Repository\nHINT: check the name\n",
        );
        let result = gh_result(&out, |_| unreachable!("formatter must not run on failure"));
        assert!(!result.success);
        assert!(!result.retryable);
        assert_eq!(result.message, "could not resolve to a Repository");
    }

    #[test]
    fn empty_issue_list_reads_naturally() {
        let out = output(0, "\n", "");
        let result = gh_result(&out, |list| {
            let list = list.trim();
            if list.is_empty() {
                "No open issues.".into()
            } else {
                format!("{} issues", list.lines().count())
            }
        });
        assert_eq!(result.message, "No open issues.");
    }

    // The confirmation gate and dispatch routing are covered in dispatch.rs and
    // schema.rs; the entity plumbing above is the connector-specific logic.
    #[allow(dead_code)]
    fn _entities_type_check() -> BTreeMap<String, String> {
        BTreeMap::new()
    }
}
