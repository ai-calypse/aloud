//! Exercises the GitHub connector against the real `gh` CLI.
//!
//! Unit tests mock the process output; this drives the actual binary, which is
//! the only way to know the argument wiring and auth check are right. Read-only
//! by default (`list_issues`), so it is safe to run.
//!
//!   cargo run -p aloud-agent --example github_connector -- <owner/repo>
//!
//! Add `create <title>` to also open an issue (a write — it will really post):
//!
//!   cargo run -p aloud-agent --example github_connector -- <owner/repo> create "Test from Aloud"

use aloud_agent::{Connector, GitHubConnector, ParsedAction};
use std::collections::BTreeMap;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let repo = args.next().unwrap_or_else(|| {
        eprintln!("usage: github_connector <owner/repo> [create <title>]");
        std::process::exit(2);
    });

    let connector = GitHubConnector::new();
    println!("gh ready: {}", connector.is_ready().await);

    match args.next().as_deref() {
        Some("create") => {
            let title = args.next().unwrap_or_else(|| "Test issue from Aloud".into());
            let action = action("github.create_issue", &[("repo", &repo), ("title", &title)]);
            report(connector.execute(&action).await);
        }
        _ => {
            let action = action("github.list_issues", &[("repo", &repo)]);
            report(connector.execute(&action).await);
        }
    }
}

fn action(name: &str, entities: &[(&str, &str)]) -> ParsedAction {
    ParsedAction {
        action: name.into(),
        app: "github".into(),
        entities: entities
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
        confidence: 1.0,
        requires_confirmation: name.contains("create"),
    }
}

fn report(result: anyhow::Result<aloud_agent::ExecutionResult>) {
    match result {
        Ok(r) => println!("success={}\n{}", r.success, r.message),
        Err(e) => println!("error: {e:#}"),
    }
}
