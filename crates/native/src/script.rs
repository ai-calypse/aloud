//! The one privileged edge of this crate: shelling out to `osascript`.
//!
//! A subprocess, so it is safe off the main thread — unlike the TIS/TSM
//! keyboard APIs that `aloud_paste` has to marshal onto the main queue. The
//! fork *is* the feature; do not reach for `objc2` to save it.

use std::process::Command;
use std::time::Duration;

/// osascript can hang forever if the target app throws a modal dialog. This
/// caps the wait; a real action returns in well under a second.
const TIMEOUT: Duration = Duration::from_secs(20);

/// Why an AppleScript run failed. `NotPermitted` is the one that matters: it is
/// a missing TCC grant, not a failed action, and the UI must offer the settings
/// pane rather than reporting a broken tool.
#[derive(Debug)]
pub enum ScriptError {
    /// First control of `app` was denied (-1743) or the prompt timed out
    /// (-1712). Either way the fix is the same Automation grant.
    NotPermitted { app: String },
    /// Anything else osascript reported.
    Failed(String),
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScriptError::NotPermitted { app } => write!(
                f,
                "Aloud needs permission to control {app} — grant it under System \
                 Settings → Privacy & Security → Automation, then try again"
            ),
            ScriptError::Failed(msg) => write!(f, "{msg}"),
        }
    }
}

/// Runs an AppleScript and returns its stdout, trimmed. `app` names the target
/// so a -1743 denial can say which app needs the grant.
///
/// ponytail: a hard 20s timeout implemented by killing the child, not
/// `wait_timeout` — pulling in a crate for one call site is not worth it.
pub fn run(app: &str, source: &str) -> Result<String, ScriptError> {
    osascript(app, &["-e", source])
}

/// Runs an AppleScript whose `on run argv` takes `args`. Like [`run_jxa`], the
/// data travels in argv and never touches the script source.
pub fn run_args(app: &str, source: &str, args: &[&str]) -> Result<String, ScriptError> {
    let mut argv = vec!["-e", source, "--"];
    argv.extend(args);
    osascript(app, &argv)
}

/// Runs a JavaScript for Automation `body` with `arg` available as `ARG`, and
/// returns its stdout. Ported from eliotshea/notes-mcp's `runJxa` (MIT): the
/// argument travels as one JSON string in argv, so user text never touches the
/// script source and needs no escaping. The `--` (macos-automator-mcp #295)
/// keeps a value starting with `-`, like a markdown list, from being read as
/// an osascript option.
pub fn run_jxa(app: &str, body: &str, arg: &serde_json::Value) -> Result<String, ScriptError> {
    let source = format!("function run(argv) {{ const ARG = JSON.parse(argv[0]); {body} }}");
    osascript(
        app,
        &["-l", "JavaScript", "-e", &source, "--", &arg.to_string()],
    )
}

fn osascript(app: &str, args: &[&str]) -> Result<String, ScriptError> {
    let mut child = Command::new("osascript")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| ScriptError::Failed(format!("spawning osascript: {e}")))?;

    // ponytail: naive poll-and-sleep timeout. A modal dialog from the target
    // app is the only thing that hangs osascript, and killing it is the right
    // response; upgrade to `wait-timeout` if this ever needs to be precise.
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                return Err(ScriptError::Failed(format!(
                    "osascript timed out controlling {app}"
                )));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(ScriptError::Failed(format!("waiting on osascript: {e}"))),
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|e| ScriptError::Failed(format!("reading osascript output: {e}")))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    // First control of an app raises a one-time Automation prompt. A denial is
    // -1743 (not authorized); an unanswered prompt times out as -1712. Both are
    // fixed by the same grant, so both map to NotPermitted. Same detection as
    // aloud_agent_spawn, but typed rather than a bail! string.
    if stderr.contains("-1743") || stderr.contains("-1712") {
        return Err(ScriptError::NotPermitted {
            app: app.to_string(),
        });
    }
    Err(ScriptError::Failed(stderr.trim().to_string()))
}

/// Escapes a string for embedding inside an AppleScript double-quoted literal.
/// Only `\` and `"` are special there. Without this, a note body containing a
/// quote would truncate the script or inject syntax.
pub fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_handles_quotes_and_backslashes() {
        assert_eq!(escape(r#"say "hi""#), r#"say \"hi\""#);
        assert_eq!(escape(r"a\b"), r"a\\b");
        assert_eq!(escape("plain"), "plain");
    }

    /// The whole point of passing JSON in argv: a note body that starts with a
    /// dash, quotes, newlines and non-ASCII must arrive byte for byte. No app is
    /// scripted, so this needs no Automation grant.
    #[cfg(target_os = "macos")]
    #[test]
    fn json_arg_survives_leading_dash_quotes_newlines_and_unicode() {
        let text = "- flour \"00\"\n- eggs\\ & <butter> — 🎂 ok";
        let out = run_jxa(
            "test",
            "return ARG.text;",
            &serde_json::json!({ "text": text }),
        )
        .unwrap();
        assert_eq!(out, text);
    }

    #[test]
    fn not_permitted_message_names_the_app() {
        let msg = ScriptError::NotPermitted {
            app: "Music".into(),
        }
        .to_string();
        assert!(msg.contains("Music"));
        assert!(msg.contains("Automation"));
    }
}
