//! Layer 2: driving apps by their real Accessibility elements.
//!
//! Layer 1 (`aloud_native`) is a named tool per action, which is deterministic
//! and gate-able but needs code per workflow. This is the general half: read the
//! *actual* buttons and fields of whatever app is focused, then act on them. No
//! workflow is pre-defined, which is the whole point — see `COMPUTER_USE.md`.
//!
//! The mechanism is two vendored MacosUseSDK CLIs (MIT, `vendor/MacosUseSDK`),
//! run one-shot per call. A subprocess rather than `objc2` bindings for the same
//! reason `aloud_native` shells to osascript: AX calls are main-thread-affine
//! in-process, and a separate process sidesteps our `on_main_thread` marshalling
//! entirely. The fork is the feature.
//!
//! macOS-only in substance; every other platform gets an empty tool list.

mod helper;
mod tree;

use serde_json::{json, Value};

pub use helper::HelperError;

/// Read the focused app's clickable elements. Read-only, so it never gates.
pub const AX_READ: &str = "ALOUD_SCREEN_READ";
/// Click an element by its coordinates, which come from [`AX_READ`].
pub const AX_CLICK: &str = "ALOUD_SCREEN_CLICK";
/// Type text into whatever has keyboard focus.
pub const AX_TYPE: &str = "ALOUD_SCREEN_TYPE";
/// Press a key chord — `return`, `cmd+s`, `escape`.
pub const AX_KEY: &str = "ALOUD_SCREEN_KEY";

/// A tool offered to the model. Mirrors `aloud_native::NativeTool` rather than
/// sharing it: `aloud_agent` depends on both to dispatch, so a shared type would
/// need a fourth crate to hold four fields. The caller maps it at the concat
/// site, exactly as it already does for native tools.
#[derive(Debug, Clone)]
pub struct ComputerTool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub read_only: bool,
}

/// The computer-use tools, or empty when the helpers are not available — an
/// unbuilt or unbundled helper must offer zero tools rather than tools that can
/// only fail, the same rule `aloud_native` applies to an empty Shortcuts library.
pub fn tools() -> Vec<ComputerTool> {
    if !cfg!(target_os = "macos")
        || helper::traversal().is_none()
        || helper::input_helper().is_none()
    {
        return Vec::new();
    }

    vec![
        ComputerTool {
            name: AX_READ.to_string(),
            description:
                "Read the on-screen controls of the app the user is currently looking at — \
                 every button, field, checkbox and link, with its exact position. Use this \
                 when no other tool covers what was asked and you need to drive the app's \
                 interface directly. Returns a numbered list; each entry's x and y are what \
                 you pass to the click tool. Call this again after any click or typing that \
                 changes the screen, because the positions move."
                    .to_string(),
            parameters: json!({ "type": "object", "properties": {} }),
            read_only: true,
        },
        ComputerTool {
            name: AX_CLICK.to_string(),
            description:
                "Click an on-screen control at the coordinates reported by the screen-read \
                 tool. Only ever pass x and y copied from a screen-read result — never \
                 guess a position, and never reuse coordinates from before the screen last \
                 changed."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "required": ["x", "y"],
                "properties": {
                    "x": { "type": "integer", "description": "The x from a screen-read entry." },
                    "y": { "type": "integer", "description": "The y from a screen-read entry." },
                    "label": {
                        "type": "string",
                        "description":
                            "The text of the control you mean to click, copied from the \
                             screen-read entry. Shown to the user in the confirmation.",
                    },
                },
            }),
            read_only: false,
        },
        ComputerTool {
            name: AX_TYPE.to_string(),
            description:
                "Type text into the field that currently has keyboard focus. Click the field \
                 first if it is not already focused."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "required": ["text"],
                "properties": { "text": { "type": "string" } },
            }),
            read_only: false,
        },
        ComputerTool {
            name: AX_KEY.to_string(),
            description: "Press a key or key chord: 'return', 'escape', 'tab', 'delete', or a \
                 combination like 'cmd+s' or 'cmd+shift+n'. Use this for keyboard \
                 shortcuts and for submitting a field."
                .to_string(),
            parameters: json!({
                "type": "object",
                "required": ["keys"],
                "properties": {
                    "keys": {
                        "type": "string",
                        "description": "e.g. 'return', 'cmd+s', 'cmd+shift+4'.",
                    },
                },
            }),
            read_only: false,
        },
    ]
}

/// Runs a computer-use tool, or `None` when the name is not one of ours — the
/// fall-through signal, same contract as `aloud_native::execute`, so the
/// dispatch never hardcodes a prefix.
pub fn execute(name: &str, args: &Value) -> Option<Result<Value, HelperError>> {
    let result = match name {
        AX_READ => read(),
        AX_CLICK => {
            // The model must copy coordinates out of a read, so a missing one is
            // a model error worth naming rather than a silent click at (0, 0) —
            // which lands on the Apple menu.
            let (Some(x), Some(y)) = (as_i32(&args["x"]), as_i32(&args["y"])) else {
                return Some(Err(HelperError::Failed(
                    "x and y are required, and must be copied from a screen-read result".into(),
                )));
            };
            helper::input(&["click", &x.to_string(), &y.to_string()])
                .map(|_| json!({ "clicked": { "x": x, "y": y } }))
        }
        AX_TYPE => {
            let text = args["text"].as_str().unwrap_or_default();
            if text.is_empty() {
                return Some(Err(HelperError::Failed("text is required".into())));
            }
            helper::input(&["writetext", text]).map(|_| json!({ "typed": text }))
        }
        AX_KEY => {
            let keys = args["keys"].as_str().unwrap_or_default();
            if keys.is_empty() {
                return Some(Err(HelperError::Failed("keys is required".into())));
            }
            helper::input(&["keypress", keys]).map(|_| json!({ "pressed": keys }))
        }
        _ => return None,
    };
    tracing::info!(tool = name, ok = result.is_ok(), "ran computer-use tool");
    Some(result)
}

/// Traverses the focused app and returns the elements worth showing the model.
fn read() -> Result<Value, HelperError> {
    let pid = frontmost_pid().ok_or_else(|| HelperError::Failed("no app is focused".into()))?;
    let raw = helper::traverse(pid)?;
    Ok(tree::summarise(&raw))
}

/// The pid of the app the user is looking at.
///
/// Asked of System Events rather than `aloud_selection::debug::frontmost_pid`,
/// which reads `AXFocusedApplication` from the system-wide element. That read
/// requires the *calling* process to be AX-trusted and returns -25204
/// (`kAXErrorCannotComplete`) when it is not — which is every `cargo run`
/// binary, because the grant belongs to the terminal and does not inherit.
/// System Events needs no such trust and was verified to answer correctly with
/// the AX path failing on the same machine. AX stays as the fallback, since it
/// is right when the app *is* trusted and costs no subprocess.
fn frontmost_pid() -> Option<i32> {
    if let Some(pid) = aloud_selection::debug::frontmost_pid() {
        return Some(pid);
    }
    system_events_pid()
}

#[cfg(target_os = "macos")]
fn system_events_pid() -> Option<i32> {
    let out = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to unix id of first application process \
             whose frontmost is true",
        ])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[cfg(not(target_os = "macos"))]
fn system_events_pid() -> Option<i32> {
    None
}

/// `serde_json` will not coerce `"420"` to a number, and models emit both.
fn as_i32(v: &Value) -> Option<i32> {
    v.as_i64()
        .or_else(|| v.as_str()?.trim().parse().ok())
        .and_then(|n| i32::try_from(n).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_returns_none_for_other_tool_sources() {
        // The fall-through signal: Composio and native slugs are not ours.
        assert!(execute("GMAIL_SEND_EMAIL", &Value::Null).is_none());
        assert!(execute("ALOUD_FINDER_SEARCH", &Value::Null).is_none());
    }

    /// Only the read is free. A click, a keystroke and typed text all pass the
    /// loop's gate, so none of them may claim `read_only`.
    #[test]
    fn only_reading_the_screen_is_read_only() {
        for tool in tools() {
            assert_eq!(
                tool.read_only,
                tool.name == AX_READ,
                "{} has the wrong read_only",
                tool.name
            );
        }
    }

    #[test]
    fn coordinates_accept_the_string_form_models_emit() {
        assert_eq!(as_i32(&json!(420)), Some(420));
        assert_eq!(as_i32(&json!("420")), Some(420));
        assert_eq!(as_i32(&json!(" 420 ")), Some(420));
        assert_eq!(as_i32(&json!("nope")), None);
        assert_eq!(as_i32(&Value::Null), None);
    }

    /// A click with no coordinates must be refused, not silently sent to (0,0) —
    /// which is the Apple menu, i.e. a click the user never asked for.
    #[test]
    fn a_click_without_coordinates_is_refused() {
        let err = execute(AX_CLICK, &json!({ "label": "Send" }));
        assert!(matches!(err, Some(Err(HelperError::Failed(_)))));
    }
}
