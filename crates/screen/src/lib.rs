//! Screen control, the last tier of a command: when no app tool, connector,
//! shortcut or menu does what was asked, the model reads an app's real
//! accessibility controls and clicks or types into them by number.
//!
//! Driven by Cua Driver (trycua/cua `libs/cua-driver`, MIT) through its CLI,
//! which talks to the driver's running daemon. The driver owns the
//! accessibility work and its permissions; this crate names the tools and
//! translates between them and the driver's.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

pub const READ: &str = "ALOUD_SCREEN_READ";
pub const CLICK: &str = "ALOUD_SCREEN_CLICK";
pub const TYPE: &str = "ALOUD_SCREEN_TYPE";

/// Bumped by [`finish`]: the driver refuses an ended session's label for good,
/// so each run gets its own.
static RUN: AtomicU64 = AtomicU64::new(0);
/// Elements walked per read. Past this a big Electron window's tree costs more
/// prompt than it is worth; `find` narrows it instead.
const MAX_ELEMENTS: u32 = 400;
/// The tree as the model sees it, in characters (~2k tokens): every round of a
/// screen run resends it.
const MAX_TREE_CHARS: usize = 8000;
/// Between an action and the read that reports it, for the app to redraw. A
/// calibration knob: raise it if the returned tree shows the screen before
/// the click.
const SETTLE: Duration = Duration::from_millis(250);

pub struct ScreenTool {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON-schema object for the function's `parameters`.
    pub parameters: Value,
    pub read_only: bool,
}

/// The screen tools, or none when Cua Driver is not installed.
pub fn tools() -> Vec<ScreenTool> {
    if driver().is_none() {
        return Vec::new();
    }
    let app =
        json!({ "type": "string", "description": "The app's name, e.g. \"System Settings\"." });
    vec![
        ScreenTool {
            name: READ,
            description: "Lists the controls in an app's front window as numbered elements, \
                          with their roles, labels and values. Only for an app no other tool \
                          covers.",
            parameters: json!({
                "type": "object",
                "required": ["app"],
                "properties": {
                    "app": app,
                    "find": { "type": "string", "description": "Only the elements whose text contains this, e.g. \"appearance\"." },
                },
            }),
            read_only: true,
        },
        ScreenTool {
            name: CLICK,
            description: "Clicks element number `element` from the app's last screen read, then \
                          returns the app's controls as they are after the click.",
            parameters: json!({
                "type": "object",
                "required": ["app", "element"],
                "properties": { "app": app, "element": { "type": "integer" } },
            }),
            read_only: false,
        },
        ScreenTool {
            name: TYPE,
            description: "Types text into element number `element` from the app's last screen \
                          read (the focused field when omitted), pressing Return after when \
                          `submit` is true, then returns the app's controls afterwards.",
            parameters: json!({
                "type": "object",
                "required": ["app", "text"],
                "properties": {
                    "app": app,
                    "element": { "type": "integer" },
                    "text": { "type": "string" },
                    "submit": { "type": "boolean" },
                },
            }),
            read_only: false,
        },
    ]
}

pub fn is_screen_tool(name: &str) -> bool {
    matches!(name, READ | CLICK | TYPE)
}

/// The tools that change something — what the once-per-run gate asks about.
pub fn is_screen_write(name: &str) -> bool {
    matches!(name, CLICK | TYPE)
}

/// Runs a screen tool, or None when `name` is not one. Blocking: every step is
/// a driver subprocess, ~30 ms each plus the work.
pub fn execute(name: &str, args: &Value) -> Option<Value> {
    let result = match name {
        READ => read(args),
        CLICK => click(args),
        TYPE => type_text(args),
        _ => return None,
    };
    Some(result.unwrap_or_else(|error| json!({ "error": error })))
}

/// Ends the driver session after a run that drove the screen, so its snapshots
/// and agent cursor go with it.
pub fn finish() {
    let ended = session();
    RUN.fetch_add(1, Ordering::Relaxed);
    if let Err(e) = call("end_session", json!({ "session": ended })) {
        tracing::debug!(%e, "could not end the screen session");
    }
    if let Ok(mut windows) = WINDOWS.lock() {
        *windows = None;
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Window {
    pid: i64,
    window_id: i64,
    /// The snapshot element numbers refer to; empty until the window is read.
    snapshot: String,
}

/// The window last read per app (lowercased), so "element 7 in Settings"
/// resolves to the snapshot it was numbered in.
static WINDOWS: Mutex<Option<HashMap<String, Window>>> = Mutex::new(None);

fn remembered(app: &str) -> Result<Window, String> {
    WINDOWS
        .lock()
        .ok()
        .and_then(|w| w.as_ref()?.get(&app.to_lowercase()).cloned())
        .ok_or_else(|| format!("read {app} with {READ} first"))
}

fn remember(app: &str, window: Window) {
    if let Ok(mut windows) = WINDOWS.lock() {
        windows
            .get_or_insert_with(HashMap::new)
            .insert(app.to_lowercase(), window);
    }
}

fn read(args: &Value) -> Result<Value, String> {
    let app = text_arg(args, "app")?;
    let window = open_window(app)?;
    snapshot(app, window, args.get("find").and_then(Value::as_str))
}

fn click(args: &Value) -> Result<Value, String> {
    let app = text_arg(args, "app")?;
    let element = args
        .get("element")
        .and_then(Value::as_u64)
        .ok_or("missing `element`")?;
    let window = remembered(app)?;
    let done = act(
        "click",
        json!({
            "pid": window.pid,
            "window_id": window.window_id,
            "element_token": token(&window, element),
            "session": session(),
        }),
    )?;
    Ok(after(app, window, done))
}

fn type_text(args: &Value) -> Result<Value, String> {
    let app = text_arg(args, "app")?;
    let text = text_arg(args, "text")?;
    let window = remembered(app)?;
    let mut params = json!({
        "pid": window.pid,
        "window_id": window.window_id,
        "text": text,
        "session": session(),
    });
    if let Some(element) = args.get("element").and_then(Value::as_u64) {
        params["element_token"] = token(&window, element).into();
    }
    let done = act("type_text", params)?;
    if args.get("submit").and_then(Value::as_bool) == Some(true) {
        act(
            "press_key",
            json!({
                "pid": window.pid,
                "window_id": window.window_id,
                "key": "return",
                "session": session(),
            }),
        )?;
    }
    Ok(after(app, window, done))
}

/// The action's outcome plus a fresh read: what the model needs next is the
/// screen the action left, and returning it here saves a round per step.
fn after(app: &str, window: Window, done: Value) -> Value {
    std::thread::sleep(SETTLE);
    // A click can close the window it was in (a dialog's OK), so a gone window
    // is looked up again rather than reported as a failure of the click.
    let now = snapshot(app, window, None)
        .or_else(|_| open_window(app).and_then(|w| snapshot(app, w, None)))
        .unwrap_or_else(|e| json!({ "error": e }));
    json!({ "done": done["summary"], "effect": done["effect"], "now": now })
}

fn snapshot(app: &str, window: Window, find: Option<&str>) -> Result<Value, String> {
    let mut params = json!({
        "pid": window.pid,
        "window_id": window.window_id,
        "include_screenshot": false,
        "max_elements": MAX_ELEMENTS,
        "session": session(),
    });
    if let Some(find) = find.filter(|f| !f.trim().is_empty()) {
        params["query"] = find.into();
    }
    let state = call("get_window_state", params)?;
    if let Some(refused) = refusal(&state) {
        return Err(refused);
    }
    let snapshot = state["snapshot_id"]
        .as_str()
        .ok_or("the driver returned no snapshot")?
        .to_string();
    remember(app, Window { snapshot, ..window });
    let mut out = json!({
        "app": state["app_name"],
        "window": state["window_title"],
        "elements": clip(state["tree_markdown"].as_str().unwrap_or_default(), MAX_TREE_CHARS),
    });
    if let Some(reason) = state["degraded_reason"].as_str() {
        out["degraded"] = reason.into();
    }
    Ok(out)
}

/// Runs a driver action. A refusal that names the foreground route (Electron
/// web content will not take background input) is retried that way once.
fn act(tool: &str, mut params: Value) -> Result<Value, String> {
    let result = call(tool, params.clone())?;
    match refusal(&result) {
        None => Ok(result),
        Some(message) if message.contains("foreground") => {
            params["delivery_mode"] = "foreground".into();
            let retried = call(tool, params)?;
            refusal(&retried).map_or(Ok(retried), Err)
        }
        Some(message) => Err(message),
    }
}

fn open_window(app: &str) -> Result<Window, String> {
    let windows = call("list_windows", json!({}))?;
    pick_window(&windows, app).ok_or_else(|| format!("{app} has no window open; open it first"))
}

/// The window to drive for `app`: its own, on screen before off, titled before
/// untitled (an untitled one is usually a panel or a helper), in the driver's
/// front-to-back order otherwise.
fn pick_window(list: &Value, app: &str) -> Option<Window> {
    let mut candidates: Vec<&Value> = list["windows"]
        .as_array()?
        .iter()
        .filter(|w| {
            w["app_name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(app.trim()))
                && w["layer"].as_i64().unwrap_or(0) == 0
        })
        .collect();
    candidates.sort_by_key(|w| {
        (
            !w["is_on_screen"].as_bool().unwrap_or(false),
            w["title"].as_str().unwrap_or_default().is_empty(),
        )
    });
    let w = candidates.first()?;
    Some(Window {
        pid: w["pid"].as_i64()?,
        window_id: w["window_id"].as_i64()?,
        snapshot: String::new(),
    })
}

/// The driver session for this run. Snapshots — and so element numbers —
/// belong to a session, and each CLI call is otherwise a session of its own,
/// which made every click "stale" against the read before it. The pid keeps a
/// relaunched app off a label the last process ended.
fn session() -> String {
    format!(
        "aloud-{}-{}",
        std::process::id(),
        RUN.load(Ordering::Relaxed)
    )
}

fn token(window: &Window, element: u64) -> String {
    format!("{}:{element}", window.snapshot)
}

/// The driver's reason for not doing something, when it did not.
fn refusal(result: &Value) -> Option<String> {
    if result["status"] == "refused" {
        return Some(
            result["refusal"]["message"]
                .as_str()
                .unwrap_or("the driver refused")
                .to_string(),
        );
    }
    result["error"].as_str().map(str::to_string)
}

fn text_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("missing `{key}`"))
}

/// Cut on a char boundary, saying so: a tree that stops mid-list with no
/// marker reads as the whole window.
fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        None => text.to_string(),
        Some((at, _)) => format!(
            "{}\n… (cut here — read again with `find` to see the rest)",
            &text[..at]
        ),
    }
}

fn call(tool: &str, params: Value) -> Result<Value, String> {
    let driver = driver().ok_or("Cua Driver is not installed")?;
    let out = Command::new(driver)
        .args(["call", tool, &params.to_string()])
        .output()
        .map_err(|e| format!("could not run Cua Driver: {e}"))?;
    serde_json::from_slice(&out.stdout).map_err(|_| {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let said = match stderr.trim() {
            "" => String::from_utf8_lossy(&out.stdout).trim().to_string(),
            stderr => stderr.to_string(),
        };
        format!("Cua Driver {tool} failed: {said}")
    })
}

/// The driver binary, by absolute path: an app opened from Finder has a PATH
/// without `~/.local/bin`.
fn driver() -> Option<&'static PathBuf> {
    static DRIVER: OnceLock<Option<PathBuf>> = OnceLock::new();
    DRIVER
        .get_or_init(|| {
            let home = std::env::var_os("HOME").map(PathBuf::from);
            // ponytail: macOS install locations only; the driver also ships
            // for Windows and Linux, whose paths get added with those builds.
            [
                Some(PathBuf::from(
                    "/Applications/CuaDriver.app/Contents/MacOS/cua-driver",
                )),
                home.map(|h| h.join(".local/bin/cua-driver")),
            ]
            .into_iter()
            .flatten()
            .find(|path| path.exists())
        })
        .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wrong window is a click in the wrong place: an off-screen helper or
    /// an untitled panel of the same app must lose to the window on screen.
    #[test]
    fn the_on_screen_titled_window_of_the_named_app_is_picked() {
        let list = json!({ "windows": [
            { "app_name": "TextEdit", "pid": 9, "window_id": 1, "title": "", "is_on_screen": false, "layer": 0 },
            { "app_name": "Finder", "pid": 5, "window_id": 2, "title": "Recents", "is_on_screen": true, "layer": 0 },
            { "app_name": "TextEdit", "pid": 9, "window_id": 3, "title": "", "is_on_screen": true, "layer": 0 },
            { "app_name": "TextEdit", "pid": 9, "window_id": 4, "title": "Notes.txt", "is_on_screen": true, "layer": 0 },
        ]});
        assert_eq!(pick_window(&list, "textedit").map(|w| w.window_id), Some(4));
        assert_eq!(pick_window(&list, "Mail"), None);
    }

    #[test]
    fn a_refusal_carries_the_drivers_reason() {
        let stale = json!({
            "status": "refused",
            "refusal": { "code": "stale_element_token", "message": "element_token is stale" },
        });
        assert_eq!(refusal(&stale).as_deref(), Some("element_token is stale"));
        assert_eq!(refusal(&json!({ "effect": "confirmed" })), None);
    }

    #[test]
    fn a_long_tree_is_cut_and_says_so() {
        assert_eq!(clip("short", 10), "short");
        let cut = clip("ééééé", 2);
        assert!(cut.starts_with("éé\n…"), "{cut}");
    }
}
