//! Locating and running the vendored MacosUseSDK helper binaries.
//!
//! One-shot per call: spawn, read stdout, the process exits. The SDK's CLIs are
//! built that way, and it keeps this file at a size worth reading — no socket,
//! no re-entry guard, no watchdog, no restart-on-crash. The cost is ~30ms of
//! spawn per action, which is nothing next to the model round-trip that decided
//! to call it.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

/// A traversal of a busy app takes ~700ms; typing is near-instant. The cap is
/// for a helper that wedges on an app showing a modal, the same failure
/// `aloud_native`'s osascript timeout exists for.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Why a helper run failed.
#[derive(Debug)]
pub enum HelperError {
    /// The AX API refused. Almost always a missing Accessibility grant, which is
    /// a settings problem rather than a broken tool, so it says so.
    NotPermitted,
    /// The helper binary is not on disk — not built in dev, not bundled in
    /// release. Distinct from a failure so the message can say which.
    Missing,
    Failed(String),
}

impl std::fmt::Display for HelperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HelperError::NotPermitted => write!(
                f,
                "Aloud needs Accessibility permission to control other apps — grant it \
                 under System Settings → Privacy & Security → Accessibility, then try again"
            ),
            HelperError::Missing => write!(
                f,
                "the screen-control helper is not installed in this build"
            ),
            HelperError::Failed(msg) => write!(f, "{msg}"),
        }
    }
}

/// Path to the traversal helper, or `None` when it is not on disk.
pub fn traversal() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| locate("TraversalTool")).as_ref()
}

/// Path to the input helper, or `None` when it is not on disk.
pub fn input_helper() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| locate("InputControllerTool")).as_ref()
}

/// Finds a helper binary: next to our executable first (how it ships), then the
/// vendored Swift build products (how it runs in `cargo test` and `pnpm dev`).
///
/// Tauri strips the target triple from sidecar names when bundling, so the
/// bundled name and the built name differ — both are tried rather than assuming
/// which one this build produced.
fn locate(name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
            // The un-stripped sidecar name, in case bundling left it on.
            candidates.push(dir.join(format!("{name}-{}", env!("HELPER_TARGET_TRIPLE"))));
            // macOS .app layout: MacOS/aloud → Resources/.
            candidates.push(dir.join("../Resources").join(name));
        }
    }

    // The dev path: whatever `build.rs` compiled out of vendor/MacosUseSDK.
    candidates.push(PathBuf::from(env!("HELPER_BUILD_DIR")).join(name));

    candidates.into_iter().find(|p| p.is_file())
}

/// Traverses `pid`'s accessibility tree, returning the helper's raw JSON.
///
/// `--visible-only` is not optional: the full tree of a normal app is ~270
/// elements and ~8k tokens, mostly menu items that cannot be clicked without
/// opening their menu first. Visible-only is 10-30 of them.
pub fn traverse(pid: i32) -> Result<Value, HelperError> {
    let path = traversal().ok_or(HelperError::Missing)?;
    let out = run(Command::new(path).args(["--visible-only", &pid.to_string()]))?;
    serde_json::from_str(&out)
        .map_err(|e| HelperError::Failed(format!("unreadable traversal output: {e}")))
}

/// Runs the input helper with `args` — `["click", "420", "300"]`, and so on.
pub fn input(args: &[&str]) -> Result<(), HelperError> {
    let path = input_helper().ok_or(HelperError::Missing)?;
    run(Command::new(path).args(args)).map(|_| ())
}

/// Spawns a helper and returns its stdout, or a typed error.
///
/// ponytail: the same poll-and-sleep timeout as `aloud_native::script::run`,
/// for the same reason — one more call site does not justify a `wait-timeout`
/// dependency. Worth extracting to a shared crate if a third appears.
fn run(cmd: &mut Command) -> Result<String, HelperError> {
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| HelperError::Failed(format!("spawning helper: {e}")))?;

    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                return Err(HelperError::Failed("the screen helper timed out".into()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(HelperError::Failed(format!("waiting on helper: {e}"))),
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|e| HelperError::Failed(format!("reading helper output: {e}")))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(classify(&String::from_utf8_lossy(&output.stderr)))
}

/// Maps helper stderr onto a typed error. A missing Accessibility grant is the
/// one worth separating: it is the difference between "grant this" and "this is
/// broken", and it is what every first run hits.
fn classify(stderr: &str) -> HelperError {
    // -25204 is kAXErrorCannotComplete and -25211 kAXErrorAPIDisabled; the SDK
    // also reports the denial in prose. Matching all three because which one
    // surfaces depends on whether the app is merely unresponsive or the whole
    // API is off for us.
    let denied = [
        "-25204",
        "-25211",
        "accessibility permission",
        "not trusted",
    ]
    .iter()
    .any(|needle| stderr.to_lowercase().contains(needle));
    if denied {
        return HelperError::NotPermitted;
    }
    let msg = stderr
        .lines()
        .rfind(|l| !l.starts_with("info:"))
        .unwrap_or("the screen helper failed")
        .trim();
    HelperError::Failed(msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A denial must be its own variant: the fix is a settings grant, and
    /// reporting it as a generic failure sends the user hunting for a bug.
    #[test]
    fn a_permission_denial_is_recognised_not_reported_as_a_failure() {
        assert!(matches!(
            classify("error: AXError -25211"),
            HelperError::NotPermitted
        ));
        assert!(matches!(
            classify("Accessibility permission denied"),
            HelperError::NotPermitted
        ));
        assert!(matches!(
            classify("error: no such element"),
            HelperError::Failed(_)
        ));
    }

    /// The helpers log progress to stderr on success as well as failure, so the
    /// message must come off the end rather than the start.
    #[test]
    fn the_reported_message_skips_the_helpers_progress_chatter() {
        let stderr = "info: ActionTool started.\ninfo: calling traverse\nerror: bad pid\n";
        let HelperError::Failed(msg) = classify(stderr) else {
            panic!("expected a plain failure");
        };
        assert_eq!(msg, "error: bad pid");
    }

    #[test]
    fn the_permission_message_names_the_settings_pane() {
        let msg = HelperError::NotPermitted.to_string();
        assert!(msg.contains("Accessibility"), "got: {msg}");
    }
}
