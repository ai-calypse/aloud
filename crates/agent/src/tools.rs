//! What a command is offered and told: the tools for this machine and account,
//! and the context a spoken command leans on ("add this to my note").

use crate::{ComposioClient, ToolDef};
use tracing::Instrument;

/// Selected text past this is cut: a whole document selected by accident should
/// not become the prompt.
const MAX_SELECTION: usize = 4000;

/// What a command is offered: its tools, and the connected services left out
/// because the command doesn't mention them, which the model can still load.
pub struct CommandTools {
    pub tools: Vec<ToolDef>,
    pub more: Vec<String>,
}

/// Every tool a command may use: the Mac's own actions, screen control, the
/// browser task and the connected services the command is about. Composio is
/// optional; missing or down, the command still runs on the local tools instead
/// of failing before the model is asked.
pub async fn command_tools(
    transcript: &str,
    installed: &[aloud_selection::InstalledApp],
    composio: Option<&ComposioClient>,
) -> CommandTools {
    let (mut tools, more) = match composio {
        None => (Vec::new(), Vec::new()),
        // A round trip before the model is even asked — invisible in any
        // per-tool timing, so it gets its own span.
        Some(composio) => match composio
            .tools_for(transcript, |toolkit| {
                crate::composio::concerns(toolkit, transcript)
            })
            .instrument(tracing::info_span!("tool_discovery"))
            .await
        {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!(%e, "connected services unavailable; running on local tools");
                (Vec::new(), Vec::new())
            }
        },
    };
    tools.extend(aloud_native::tools(installed).into_iter().map(|t| ToolDef {
        name: t.name,
        toolkit: "macOS".to_string(),
        description: t.description,
        parameters: t.parameters,
        read_only: t.read_only,
    }));
    // Empty when Cua Driver isn't installed.
    tools.extend(aloud_screen::tools().into_iter().map(|t| ToolDef {
        name: t.name.to_string(),
        toolkit: "Screen".to_string(),
        description: t.description.to_string(),
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
    CommandTools { tools, more }
}

/// The command as the model sees it: what was said, plus the app and window the
/// user was looking at and anything they had selected, so "add this to my
/// note", "summarise this" and "open components" have a "this" and a "here".
///
/// The window rides only with an app: no app means the front window was our own
/// panel, which says nothing about where the user is.
pub fn with_context(
    transcript: &str,
    front: Option<&str>,
    title: Option<&str>,
    document: Option<&str>,
    selection: Option<&str>,
) -> String {
    let mut command = transcript.to_string();
    if let Some(app) = front {
        command.push_str(&format!("\n(The app on screen: {app}"));
        if let Some(title) = title.map(str::trim).filter(|t| !t.is_empty()) {
            command.push_str(&format!(" — \"{title}\""));
        }
        if let Some(document) = document {
            command.push_str(&format!(", {document}"));
        }
        command.push(')');
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
        assert_eq!(with_context("hi", None, None, None, Some("  ")), "hi");
        assert_eq!(
            with_context(
                "add this to my groceries note",
                Some("Safari"),
                None,
                None,
                Some(" oat milk ")
            ),
            "add this to my groceries note\n(The app on screen: Safari)\n(Selected text: «oat milk»)"
        );
    }

    /// A select-all left over in a long document must not swamp the prompt.
    #[test]
    fn a_huge_selection_is_cut() {
        let big = "x".repeat(MAX_SELECTION * 3);
        let command = with_context("summarise this", None, None, None, Some(&big));
        assert!(command.chars().count() < MAX_SELECTION + 100);
    }

    /// "Open components" and "summarise this" lean on which window and file
    /// the user was in, not just the app.
    #[test]
    fn context_carries_the_front_window_and_its_document() {
        assert_eq!(
            with_context(
                "open components",
                Some("Cursor"),
                Some("main.rs — aloud-tauri"),
                None,
                None
            ),
            "open components\n(The app on screen: Cursor — \"main.rs — aloud-tauri\")"
        );
        assert_eq!(
            with_context(
                "summarise this",
                Some("Preview"),
                Some("report.pdf"),
                Some("/Users/me/report.pdf"),
                None
            ),
            "summarise this\n(The app on screen: Preview — \"report.pdf\", /Users/me/report.pdf)"
        );
        assert_eq!(
            with_context("hi", Some("Finder"), Some(" "), None, None),
            "hi\n(The app on screen: Finder)",
            "an untitled window adds nothing"
        );
        assert_eq!(
            with_context("hi", None, Some("Aloud"), Some("/x"), None),
            "hi",
            "no app means the window was our own panel"
        );
    }
}
