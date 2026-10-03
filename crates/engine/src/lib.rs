//! Agent tools backed by the task engine in `engine/` (TypeScript, from the voice-mac project).
//!
//! The engine runs whole multi-step website tasks — "play the latest freeCodeCamp video at 10:00" —
//! with TypeSafe's Jev choosing every click from typed probabilities, so the agent hands it one goal
//! instead of driving the page a step at a time. One tool: the user's default browser, operated through
//! accessibility by the engine's `axd` helper (no browser debugging port). Mac apps are not driven here
//! any more; `aloud_native` covers them with scripted actions.
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

/// A tool offered to the model (mapped to `aloud_agent::ToolDef` at the concat site).
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
    name == BROWSER_TASK
}

/// The browser task tool, or none when the engine isn't configured, so the model is never offered a
/// tool that can only fail.
pub fn tools() -> Vec<EngineTool> {
    if !cfg!(target_os = "macos") || engine_dir().is_none() {
        return Vec::new();
    }
    let goal = |what: &str| {
        json!({
            "type": "object",
            "required": ["goal"],
            "properties": {
                "goal": { "type": "string", "description": what },
                "query": {
                    "type": "string",
                    "description": "When the task starts with a web search: just the search terms, as the user would type them (\"cat memes\", not a sentence)."
                },
                "url": {
                    "type": "string",
                    "description": "When the user named a site or address to start at: that address."
                }
            }
        })
    };
    vec![
        EngineTool {
            name: BROWSER_TASK.to_string(),
            description: "Do a whole task on a website in the user's web browser, start to finish: \
                 open a site, search, play a video at a timestamp, read a value off a page, fill a form, \
                 open a dashboard section. Pass the user's request as one plain-language goal with every \
                 detail they gave (names, dates, numbers, which site) — their words, not an embellished \
                 sentence — plus `query` for a search or `url` for a named address. Returns what was \
                 found or done and \
                 the steps taken. Anything that would buy, send, delete or sign in asks the user first."
                .to_string(),
            parameters: goal("The complete task, e.g. \"Open YouTube, play the latest Reckless Ben video and jump to 10:00\""),
            read_only: false,
        },
    ]
}

/// Start a task for `name` (one of [`tools`]). `None` when the name isn't an engine tool — the same
/// fall-through contract as `aloud_native::execute`.
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
    // Runs in the user's default browser through accessibility, not a separate automated one.
    let start = |key: &str| args[key].as_str().map(str::trim).filter(|s| !s.is_empty());
    let params = json!({ "goal": goal, "url": start("url"), "query": start("query") });
    Some(engine::start_task(Path::new(&dir), "web.start", params).await)
}
