//! Agent mode: turn a spoken command into an executed action across apps.
//!
//! The model is given the tools the user has actually connected and decides
//! which to call; Composio owns the OAuth and the API surface behind them.
//!
//! An earlier design shipped alongside this one — a universal action schema, a
//! per-service `Connector` trait, an intent parser, and a dispatcher — on the
//! assumption that each service would need hand-written support. Composio made
//! all of it unreachable, so it was deleted rather than left to rot; `git log`
//! has it if per-service connectors are ever wanted again.
//!
//! The design principle survives both: an action never fires without explicit
//! approval, and every write requires confirmation.

pub mod cache;
pub mod composio;
pub mod tool_loop;

pub use cache::ResponseCache;
pub use composio::{toolkit_of_slug, ComposioClient, ToolDef, Toolkit, TOOLS_PER_TOOLKIT};
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
