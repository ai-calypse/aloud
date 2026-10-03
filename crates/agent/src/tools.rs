//! What a command is offered and told: the tools for this machine and account,
//! and the context a spoken command leans on ("add this to my note").

use crate::{ComposioClient, ToolDef};
use tracing::Instrument;

/// Selected text past this is cut: a whole document selected by accident should
/// not become the prompt.
const MAX_SELECTION: usize = 4000;

/// Every tool a command may use: the Mac's own actions, the browser task and the
/// user's connected services. Composio is optional and fetched alongside the
/// rest; missing or down, the command still runs on the local tools instead of
/// failing before the model is asked.
pub async fn command_tools(
    transcript: &str,
    installed: &[aloud_selection::InstalledApp],
    composio: Option<&ComposioClient>,
) -> Vec<ToolDef> {
    let remote = async {
        let Some(composio) = composio else {
            return Vec::new();
        };
        // A round trip before the model is even asked — invisible in any
        // per-tool timing, so it gets its own span.
        match composio
            .available_tools(transcript)
            .instrument(tracing::info_span!("tool_discovery"))
            .await
        {
            Ok(tools) => tools,
            Err(e) => {
                tracing::warn!(%e, "connected services unavailable; running on local tools");
                Vec::new()
            }
        }
    };
    let mut tools = remote.await;
    tools.extend(aloud_native::tools(installed).into_iter().map(|t| ToolDef {
        name: t.name,
        toolkit: "macOS".to_string(),
        description: t.description,
        parameters: t.parameters,
        read_only: t.read_only,
    }));
    // Empty when the engine wasn't staged into this build.
    tools.extend(aloud_engine::tools().into_iter().map(|t| ToolDef {
        toolkit: "Browser".to_string(),
        name: t.name,
        description: t.description,
        parameters: t.parameters,
        read_only: t.read_only,
    }));
    tools
}

/// The command as the model sees it: what was said, plus the app the user was
/// looking at and anything they had selected, so "add this to my note" and
/// "summarise this" have a "this".
pub fn with_context(transcript: &str, front: Option<&str>, selection: Option<&str>) -> String {
    let mut command = transcript.to_string();
    if let Some(app) = front {
        command.push_str(&format!("\n(The app on screen: {app})"));
    }
    if let Some(selected) = selection.map(str::trim).filter(|s| !s.is_empty()) {
        let selected: String = selected.chars().take(MAX_SELECTION).collect();
        command.push_str(&format!("\n(Selected text: «{selected}»)"));
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_carries_the_front_app_and_selection_and_skips_blanks() {
        assert_eq!(with_context("hi", None, Some("  ")), "hi");
        assert_eq!(
            with_context("add this to my groceries note", Some("Safari"), Some(" oat milk ")),
            "add this to my groceries note\n(The app on screen: Safari)\n(Selected text: «oat milk»)"
        );
    }

    /// A select-all left over in a long document must not swamp the prompt.
    #[test]
    fn a_huge_selection_is_cut() {
        let big = "x".repeat(MAX_SELECTION * 3);
        let command = with_context("summarise this", None, Some(&big));
        assert!(command.chars().count() < MAX_SELECTION + 100);
    }
}
