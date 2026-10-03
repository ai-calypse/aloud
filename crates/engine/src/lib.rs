//! Agent tools backed by the task engine in `engine/` (TypeScript, from the voice-mac project).
//!
//! The engine runs whole multi-step tasks — "play the latest freeCodeCamp video at 10:00", "add 90 and 7
//! in Calculator" — with TypeSafe's Jev choosing every click from typed probabilities, so the agent hands
//! it one goal instead of driving the screen a step at a time. Two tools: the user's default browser and
//! Mac apps, both operated through accessibility by the engine's `axd` helper (no browser debugging
//! port), with quick AppleScript commands for common Mac requests.
//!
//! One long-lived child process, newline-delimited JSON-RPC on stdio. A task streams its steps and can
//! pause for a yes/no before a risky click; [`Task`] surfaces both so the agent loop can show progress
//! and ask through its own gate. The engine exits when this process does (its stdin closes).

mod engine;

pub use engine::{Task, Update};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// A task on a website, run in the user's browser.
pub const BROWSER_TASK: &str = "ALOUD_BROWSER_TASK";
/// A task in a Mac app (or the Mac itself).
pub const MAC_TASK: &str = "ALOUD_MAC_TASK";

/// A tool offered to the model; mirrors `aloud_computer::ComputerTool` (mapped at the concat site).
#[derive(Debug, Clone)]
pub struct EngineTool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub read_only: bool,
}

/// The engine folder: `ALOUD_ENGINE_DIR`, else the copy staged into the app bundle
/// (`Contents/Resources/resources/engine`), else the repo's `engine/` in development.
fn engine_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    [
        std::env::var_os("ALOUD_ENGINE_DIR").map(PathBuf::from),
        exe_dir.map(|d| d.join("../Resources/resources/engine")),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../engine")),
    ]
    .into_iter()
    .flatten()
    .find(|d| d.join("rpc.ts").is_file() && d.join("node_modules").is_dir())
}

pub fn is_engine_tool(name: &str) -> bool {
    matches!(name, BROWSER_TASK | MAC_TASK)
}

/// The two task tools, or none when the engine isn't configured, so the model is never offered a tool
/// that can only fail (the rule `aloud_computer` follows for missing helpers).
pub fn tools() -> Vec<EngineTool> {
    if !cfg!(target_os = "macos") || engine_dir().is_none() {
        return Vec::new();
    }
    let goal = |what: &str| {
        json!({
            "type": "object",
            "required": ["goal"],
            "properties": { "goal": { "type": "string", "description": what } }
        })
    };
    vec![
        EngineTool {
            name: BROWSER_TASK.to_string(),
            description: "Do a whole task on a website in the user's web browser, start to finish: \
                 open a site, search, play a video at a timestamp, read a value off a page, fill a form, \
                 open a dashboard section. Pass the user's request as one plain-language goal with every \
                 detail they gave (names, dates, numbers, which site). Returns what was found or done and \
                 the steps taken. Anything that would buy, send, delete or sign in asks the user first."
                .to_string(),
            parameters: goal("The complete task, e.g. \"Open YouTube, play the latest Reckless Ben video and jump to 10:00\""),
            read_only: false,
        },
        EngineTool {
            name: MAC_TASK.to_string(),
            description: "Do a whole task in a Mac app that has no dedicated tool here, start to \
                 finish, by operating the app's own buttons, fields and menus: Calculator, TextEdit, \
                 System Settings, Finder windows, Notes or Reminders screens, any third-party app. Pass \
                 one plain-language goal naming the app. Prefer the dedicated macOS tools when one fits; \
                 this is for everything else. Risky actions ask the user first."
                .to_string(),
            parameters: goal("The complete task, e.g. \"In Calculator, calculate 12 times 3\""),
            read_only: false,
        },
    ]
}

/// Route a spoken command straight to the engine, skipping the agent's language model: one Jev request
/// (~0.15 s) decides Mac app, website, or neither. `None` means "not ours" (no engine in this build, or a
/// connected-service request, a plain question, or an unsure route) and the agent loop should take it.
///
/// `front` is the app the user was looking at when they spoke, read before this process's own panel
/// took focus; it is what lets a follow-up like "what's 90 + 7" run in the Calculator already open.
pub async fn command(text: &str, front: Option<&str>) -> Option<Result<Task, String>> {
    let dir = engine_dir()?;
    engine::command(&dir, text, front).await.transpose()
}

/// Start a task for `name` (one of [`tools`]). `None` when the name isn't an engine tool — the same
/// fall-through contract as `aloud_computer::execute`.
pub async fn start(name: &str, args: &Value) -> Option<Result<Task, String>> {
    if !is_engine_tool(name) {
        return None;
    }
    let goal = args["goal"].as_str().unwrap_or_default().trim().to_string();
    if goal.is_empty() {
        return Some(Err("missing required argument \"goal\"".to_string()));
    }
    let Some(dir) = engine_dir() else {
        return Some(Err(
            "the task engine isn't in this build (engine/ wasn't staged)".to_string(),
        ));
    };
    // Web tasks run in the user's default browser through accessibility, like any Mac app.
    let params = json!({ "goal": goal, "web": name == BROWSER_TASK });
    let method = "mac.start";
    Some(engine::start_task(Path::new(&dir), method, params).await)
}
