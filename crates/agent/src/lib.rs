//! Agent mode: turn a spoken command into an executed action across apps.
//!
//! The model is given the tools the user has actually connected and decides
//! which to call; Composio owns the OAuth and the API surface behind them.
//!
//! The invariant throughout: an action never fires without explicit approval,
//! and every write requires confirmation.

pub mod cache;
pub mod composio;
pub mod preview;
pub mod tool_loop;
pub mod tools;

pub use cache::ResponseCache;
pub use composio::{toolkit_of_slug, ComposioClient, ToolDef, Toolkit, TOOLS_PER_TOOLKIT};
pub use tool_loop::{
    first_turn, run as run_tool_loop, Callbacks, Command, LoopEvent, Mock, ProposedCall, Verdict,
};
pub use tools::{command_tools, with_context, CommandTools};

/// A model the command loop runs on, with the client that reaches it. The loop
/// takes these best first and moves down on the first round when one is out of
/// budget or unpaid; see `tool_loop::fails_over`.
pub struct Brain {
    pub client: aloud_llm::LlmClient,
    pub model: &'static str,
}

/// First choice: on the same tool call qwen answered in 0.34s against Gemini's
/// ~1.1s, and this key allows 450 requests a minute (gpt-oss on Cerebras was as
/// fast but capped at 5). Measured 2026-10-03.
const CEREBRAS: (&str, &str) = ("https://api.cerebras.ai/v1", "qwen-3.8-27b");
/// Behind it: headroom when Cerebras's per-minute budget trips. Flash-Lite, not
/// Flash: on a free-tier key 3.5 Flash allows 20 requests a day, which one eval
/// run exhausted for 14 hours; Flash-Lite answered in 0.58s on the same key.
const GEMINI: (&str, &str) = (
    "https://generativelanguage.googleapis.com/v1beta/openai",
    "gemini-3.5-flash-lite",
);

/// One named brain on its own, for the eval to compare them: "cerebras" or "gemini".
pub fn brain(name: &str, key: &str) -> anyhow::Result<Brain> {
    let (base_url, model) = match name {
        "cerebras" => CEREBRAS,
        "gemini" => GEMINI,
        other => anyhow::bail!("no brain called {other:?}"),
    };
    Ok(Brain {
        client: aloud_llm::LlmClient::new(key, Some(base_url))?,
        model,
    })
}

/// The brains a command may use, best first, for whichever keys are set.
pub fn brains(cerebras_key: &str, gemini_key: &str) -> anyhow::Result<Vec<Brain>> {
    [(cerebras_key, CEREBRAS), (gemini_key, GEMINI)]
        .into_iter()
        .filter(|(key, _)| !key.trim().is_empty())
        .map(|(key, (base_url, model))| {
            Ok(Brain {
                client: aloud_llm::LlmClient::new(key, Some(base_url))?,
                model,
            })
        })
        .collect()
}

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
