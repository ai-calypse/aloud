//! Agent mode: turn a spoken command into an executed action across apps.
//!
//! The MVP build plan's Phase 0 and 1 foundations: a universal action schema,
//! a uniform connector interface, an intent parser, and a dispatcher with the
//! Phase 4 robustness pieces (retry, honest gaps, chained partial-failure).
//!
//! Connectors (Spotify, Slack, ...) plug into the [`dispatch::Dispatcher`];
//! none ship yet — each needs its own OAuth setup.
//!
//! The design principle from the plan: an action never fires without explicit
//! approval, and every write requires confirmation.

pub mod composio;
pub mod connector;
pub mod dispatch;
pub mod github;
pub mod parser;
pub mod schema;
pub mod tool_loop;

pub use composio::{ComposioClient, ToolDef};
pub use connector::{Connector, ExecutionResult, TokenStore};
pub use dispatch::{dispatch_chain, Dispatcher};
pub use github::GitHubConnector;
pub use parser::parse_intent;
pub use schema::{ParsedAction, ACTIONS};
pub use tool_loop::{run as run_tool_loop, LoopEvent};

/// Whether a command is an agent request or plain dictation.
///
/// Task 3: agent mode is entered explicitly, not inferred from content — a
/// dedicated hotkey, or the spoken trigger below as a convenience. Mirrors the
/// ask-trigger design so "hey flow" and "hey agent" feel consistent.
pub fn command_from_trigger(transcript: &str) -> Option<String> {
    const TRIGGERS: &[&[&[&str]]] = &[&[&["hey"], &["agent", "agents"]], &[&["do"], &["this"]]];

    let words: Vec<&str> = transcript.split_whitespace().collect();
    for phrase in TRIGGERS {
        if words.len() <= phrase.len() {
            continue;
        }
        let matches = words
            .iter()
            .zip(*phrase)
            .all(|(word, spellings)| spellings.contains(&normalize(word).as_str()));
        if !matches {
            continue;
        }
        let command = words[phrase.len()..]
            .join(" ")
            .trim_start_matches([',', '.', ' '])
            .to_string();
        return (!command.is_empty()).then_some(command);
    }
    None
}

fn normalize(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_command_after_the_trigger() {
        assert_eq!(
            command_from_trigger("Hey agent, play some jazz").as_deref(),
            Some("play some jazz")
        );
        assert_eq!(
            command_from_trigger("do this: create a github issue").as_deref(),
            Some("create a github issue")
        );
    }

    #[test]
    fn plain_dictation_is_not_a_command() {
        assert_eq!(command_from_trigger("send an email to the team"), None);
        assert_eq!(command_from_trigger("hey agent"), None); // no command follows
    }
}
