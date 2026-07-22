//! Agent mode: turn a spoken command into an executed action across apps.
//!
//! The model is given the tools the user has actually connected and decides
//! which to call; Composio owns the OAuth and the API surface behind them.
//!
//! The invariant throughout: an action never fires without explicit approval,
//! and every write requires confirmation.

pub mod cache;
pub mod composio;
pub mod tool_loop;

pub use cache::ResponseCache;
pub use composio::{toolkit_of_slug, ComposioClient, ToolDef, Toolkit, TOOLS_PER_TOOLKIT};
pub use tool_loop::{run as run_tool_loop, LoopEvent};

/// Whether a command is an agent request or plain dictation. Agent mode is
/// entered explicitly, never inferred from content — a dedicated hotkey, or
/// these spoken triggers as a convenience.
pub fn command_from_trigger(transcript: &str) -> Option<String> {
    const TRIGGERS: &[&[&[&str]]] = &[&[&["hey"], &["agent", "agents"]], &[&["do"], &["this"]]];
    aloud_llm::trigger::after_trigger(transcript, TRIGGERS)
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
