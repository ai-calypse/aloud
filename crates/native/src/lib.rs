//! Native macOS actions as agent tools — the local half of agent mode.
//!
//! Composio covers anything with a cloud API; this covers what does not — play
//! music, add a reminder, reveal a file. Same tool shape the loop already
//! consumes, so it plugs into the existing gate, dedup, and budgets with one
//! dispatch branch and no new architecture. See `freeflow-agent-mode-mvp-plan`.
//!
//! macOS-only in substance; every other platform gets an empty tool list, the
//! way `notch.rs` and `aloud_volume` do.

mod catalogue;
pub mod fast;
mod media;
mod notes;
mod organize;
mod reminders;
mod script;
mod search;
mod shortcuts;
mod thumbnail;

use serde_json::{json, Value};

pub use script::ScriptError;

/// A file preview for the notch grid — a base64 `data:` URI, or None when Quick
/// Look has no preview. The command layer maps search paths through this.
pub fn thumbnail(path: &str) -> Option<String> {
    thumbnail::data_uri(path)
}

/// The single tool that fronts the whole Shortcuts library. Enumerated, not one
/// tool per shortcut, and absent entirely when the library is empty — which is
/// most machines.
const RUN_SHORTCUT: &str = "ALOUD_RUN_SHORTCUT";

/// A native tool offered to the model. Deliberately *not* `aloud_agent::ToolDef`:
/// `aloud_agent` depends on this crate to dispatch, so depending back would
/// cycle. The caller maps this into `ToolDef` at the concat site — a three-field
/// copy, cheaper than a shared crate for two structs.
#[derive(Debug, Clone)]
pub struct NativeTool {
    pub name: String,
    pub description: String,
    /// JSON-schema object for the function's `parameters`.
    pub parameters: Value,
    pub read_only: bool,
}

/// The native tools available on this machine, filtered to apps that are
/// actually installed — the local mirror of Composio's connected-toolkits idea.
/// No Music on this Mac, no Music tools; keeps the schema token count down too.
///
/// Off macOS this is always empty.
pub fn tools(installed: &[aloud_selection::InstalledApp]) -> Vec<NativeTool> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = installed;
        return Vec::new();
    }
    #[cfg(target_os = "macos")]
    {
        let mut tools: Vec<NativeTool> = catalogue::ACTIONS
            .iter()
            .filter(|action| app_installed(action.app, installed))
            .map(|action| NativeTool {
                name: action.name.to_string(),
                description: action.description.to_string(),
                parameters: (action.params)(),
                read_only: action.read_only,
            })
            .collect();

        // One tool for the whole Shortcuts library, and only when it is
        // non-empty — an empty `shortcuts list` must yield zero tools, not one
        // that can only fail. The available names go in the schema so the model
        // picks a real one.
        let library = shortcuts::list();
        if !library.is_empty() {
            tools.push(NativeTool {
                name: RUN_SHORTCUT.to_string(),
                description: format!(
                    "Run one of the user's Shortcuts by name. Available: {}.",
                    library.join(", ")
                ),
                parameters: json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "enum": library }
                    }
                }),
                // A shortcut can do anything; it must pass the gate.
                read_only: false,
            });
        }

        tools
    }
}

/// Runs a native tool by name. Returns `None` when `name` is not one of ours,
/// which is the signal for the dispatch branch to fall through to Composio —
/// so the `ALOUD_` prefix is never hardcoded at the call site.
///
/// The `Ok` value is a JSON result in the same shape Composio returns, so the
/// loop's `tool_succeeded` and the panel need no special case. A `ScriptError`
/// is surfaced as an error result the model can read back.
/// Writes too small to confirm: each is undone by saying the opposite (open/quit, play/pause, volume,
/// a window moved, a reminder or a note line added, the clipboard set). Asking "are you sure?" before
/// opening Safari made every command a two-step chore. Sending, quitting, Shortcuts and file moves or
/// writes are left out, so they still ask.
pub fn low_risk(name: &str) -> bool {
    matches!(
        name,
        "ALOUD_OPEN_APP"
            | "ALOUD_MUSIC"
            | "ALOUD_SPOTIFY"
            | "ALOUD_REMINDERS_ADD"
            | "ALOUD_NOTES_APPEND"
            | "ALOUD_NOTES_CREATE"
            | "ALOUD_WINDOW_ARRANGE"
            | "ALOUD_BROWSER_TAB"
            | "ALOUD_CLIPBOARD"
            | "ALOUD_FINDER_REVEAL"
            | "ALOUD_SYSTEM_VOLUME"
    )
}

pub fn execute(name: &str, args: &Value) -> Option<Result<Value, ScriptError>> {
    if name == RUN_SHORTCUT {
        tracing::info!(tool = name, "running native tool");
        let shortcut = args.get("name").and_then(Value::as_str).unwrap_or_default();
        if shortcut.is_empty() {
            return Some(Err(ScriptError::Failed("missing shortcut name".into())));
        }
        return Some(match shortcuts::run(shortcut) {
            Ok(out) => Ok(json!({ "ran": shortcut, "output": out })),
            Err(e) => Err(ScriptError::Failed(e)),
        });
    }
    let action = catalogue::ACTIONS.iter().find(|a| a.name == name)?;
    tracing::info!(tool = name, "running native tool");
    Some((action.run)(args))
}

/// Sentinel `app` for tools backed by macOS itself, not a bundled app —
/// Spotlight search, output volume, revealing in Finder. These are always
/// present, so they are never gated. Using a real app name here would be wrong:
/// Finder lives in CoreServices and never appears in `installed_apps()`, so
/// gating on it silently dropped every Spotlight/volume/reveal tool.
pub(crate) const ALWAYS: &str = "macOS";

/// Whether a tool's backing app is available. The `ALWAYS` sentinel is
/// unconditionally true; otherwise a case-insensitive match on an installed
/// app's display name — the same names `frontmost_app()` reports, so "Music"
/// matches the app the user would name out loud.
#[cfg(target_os = "macos")]
fn app_installed(app: &str, installed: &[aloud_selection::InstalledApp]) -> bool {
    app == ALWAYS || installed.iter().any(|a| a.name.eq_ignore_ascii_case(app))
}

#[cfg(test)]
mod tests {

    #[test]
    fn sending_quitting_and_file_moves_still_ask_first() {
        for name in [
            "ALOUD_MESSAGES_SEND",
            "ALOUD_QUIT_APP",
            "ALOUD_COLLECT_FILES",
            "ALOUD_WRITE_FILE",
            "ALOUD_ORGANIZE_FOLDER",
        ] {
            assert!(!low_risk(name), "{name} must keep its confirmation");
        }
        assert!(low_risk("ALOUD_OPEN_APP"));
    }
    use super::*;
    use aloud_selection::InstalledApp;

    fn app(name: &str) -> InstalledApp {
        InstalledApp {
            name: name.to_string(),
            bundle_id: None,
            icon: None,
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn app_gated_tools_need_their_app() {
        // No Music installed → no Music tools.
        let names =
            |apps: &[InstalledApp]| tools(apps).into_iter().map(|t| t.name).collect::<Vec<_>>();
        assert!(!names(&[]).iter().any(|n| n == "ALOUD_MUSIC"));
        // Music installed → its tools appear.
        assert!(names(&[app("Music")]).iter().any(|n| n == "ALOUD_MUSIC"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn system_wide_tools_are_always_offered() {
        // Spotlight/volume/reveal are built into macOS, so they must appear even
        // with zero apps installed. Regression: they were gated on "Finder",
        // which never appears in installed_apps(), so they silently vanished and
        // "find images" fell through to Composio.
        let names: Vec<_> = tools(&[]).into_iter().map(|t| t.name).collect();
        assert!(names.iter().any(|n| n == "ALOUD_FINDER_SEARCH"));
        assert!(names.iter().any(|n| n == "ALOUD_FINDER_REVEAL"));
        assert!(names.iter().any(|n| n == "ALOUD_SYSTEM_VOLUME"));
    }

    #[test]
    fn execute_returns_none_for_unknown_tools() {
        // The fall-through signal: a Composio slug is not ours.
        assert!(execute("GMAIL_SEND_EMAIL", &Value::Null).is_none());
    }
}
