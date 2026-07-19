//! Agent mode: run a spoken command across apps.
//!
//! The flow, ported from the agent crate's design principle "an action never
//! fires without explicit approval":
//!
//!   transcribe → parse intent → (confirm if it writes) → dispatch → report
//!
//! Reads run straight through; writes surface a confirmation panel and wait
//! for a yes. The panel mirrors `ask.rs`: a focused, always-on-top window that
//! Escape dismisses. The difference is it collects a decision, so it holds a
//! one-shot channel that the approve/reject commands resolve.

use aloud_store::Settings;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::sync::oneshot;

const LABEL: &str = "agent";
/// How long a write waits for a yes before giving up and declining.
const CONFIRM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Holds the pending confirmation's reply channel, so the frontend's
/// approve/reject command can wake the awaiting dispatch. Only one confirmation
/// is ever in flight — a new agent command replaces any stale pending one.
#[derive(Default)]
pub struct PendingConfirmation(Mutex<Option<oneshot::Sender<bool>>>);

/// Streamed to the agent panel.
///
/// The event vocabulary follows the AG-UI protocol's tool-call lifecycle
/// (https://docs.ag-ui.com) — a parsed action *is* a tool call, so `run` /
/// `tool-call` / `tool-result` map its shape onto AI Elements' tool renderer.
/// It is the AG-UI *vocabulary*, not the wire protocol: these stay
/// tauri-specta-typed Tauri events, not an SSE stream, because there is one
/// in-process client. Args are sent whole (not streamed as deltas) since the
/// parse returns the whole action at once.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
#[tauri_specta(event_name = "agent")]
pub enum AgentEvent {
    /// AG-UI RUN_STARTED — the transcript, echoed so the panel shows what it heard.
    RunStarted(String),
    /// AG-UI TOOL_CALL_START + ARGS — a parsed action, with its args, awaiting
    /// dispatch. A write pauses here for confirmation (AG-UI human-in-the-loop).
    ToolCall(ToolCall),
    /// AG-UI TOOL_CALL_RESULT — the dispatcher's outcome for the pending call.
    ToolResult(ToolResult),
    /// A transient status line (e.g. "rate limited, retrying in 30s") shown
    /// while the run is otherwise silent; cleared by the next event.
    Status(String),
    /// AG-UI RUN_FINISHED — the run's closing message.
    RunFinished(String),
    /// AG-UI RUN_ERROR — the run failed before producing a result.
    RunError(String),
}

/// An AG-UI tool call: the action name, the app it targets, and the parsed
/// args. Kept separate from `ParsedAction` so the UI contract does not leak
/// parser internals.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    /// The tool name, AG-UI's `toolCallName`, e.g. "github.create_issue".
    pub name: String,
    /// The target app, e.g. "github".
    pub app: String,
    /// The arguments the model chose, field by field.
    ///
    /// A map rather than pre-formatted lines so the panel can decide what a
    /// given tool is worth showing — a draft's body as prose, an inbox fetch as
    /// nothing at all — instead of dumping every field including `user_id`.
    ///
    /// Values are strings because the panel only ever renders text, and because
    /// `serde_json::Value` cannot cross specta: its number type is i64-backed,
    /// which specta forbids exporting rather than silently lose precision.
    /// Non-string values are JSON-encoded, so a nested object still displays.
    pub args: std::collections::BTreeMap<String, String>,
    /// Whether this call pauses for confirmation before it runs (every write).
    pub requires_confirmation: bool,
}

/// An AG-UI tool result: whether the call succeeded and its message.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub success: bool,
    pub message: String,
}

fn emit(app: &AppHandle, event: AgentEvent) {
    // Typed emit from the tauri-specta `Event` derive: the payload type and the
    // "agent" event name are the same ones the generated bindings listen on.
    if let Err(e) = event.emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send to agent panel");
    }
}

/// Runs the full agent flow for a transcript. Called by the pipeline.
pub async fn run(app: &AppHandle, transcript: String, settings: &Settings) -> Result<()> {
    show(app);
    emit(app, AgentEvent::RunStarted(transcript.clone()));

    match execute(app, &transcript, settings).await {
        Ok(message) => emit(app, AgentEvent::RunFinished(message)),
        Err(e) => {
            tracing::error!(%e, "agent failed");
            emit(app, AgentEvent::RunError(format!("{e:#}")));
        }
    }
    Ok(())
}

// The tool loop runs on Cerebras, not Groq: a multi-round tool loop makes
// several chat calls in a row and Groq's tokens-per-minute limit throttles it.
// Cerebras is OpenAI-compatible, so the same LlmClient works against its
// endpoint, and gpt-oss-120b supports function calling.
const CEREBRAS_BASE_URL: &str = "https://api.cerebras.ai/v1";
const CEREBRAS_MODEL: &str = "gpt-oss-120b";

async fn execute(app: &AppHandle, transcript: &str, settings: &Settings) -> Result<String> {
    if settings.cerebras_api_key.trim().is_empty() {
        bail!("Agent mode needs a Cerebras API key to understand commands — add one in Settings");
    }
    if settings.composio_api_key.trim().is_empty() {
        bail!("Agent mode needs a Composio API key to reach Gmail/GitHub — add one in Settings");
    }

    let llm = aloud_llm::LlmClient::new(&settings.cerebras_api_key, Some(CEREBRAS_BASE_URL))?;
    let composio = aloud_agent::ComposioClient::new(&settings.composio_api_key)?;

    // Discover the Gmail/GitHub tools the user actually has connected. Empty
    // means nothing is connected — say so rather than letting the model flail.
    let tools = composio.available_tools().await?;
    if tools.is_empty() {
        bail!("No Gmail or GitHub account is connected in Composio yet");
    }

    aloud_agent::run_tool_loop(
        &llm,
        CEREBRAS_MODEL,
        &composio,
        transcript,
        &tools,
        // Each tool call surfaces on the panel as it happens (AG-UI lifecycle).
        |event| emit(app, event_of(event)),
        // A write tool waits for an explicit yes; this is the safety gate.
        |name, args| confirm_write(app, name, args),
    )
    .await
}

/// Surfaces a write tool for confirmation and awaits the panel's decision. Takes
/// owned args so the returned future doesn't borrow the loop's transient data.
async fn confirm_write(app: &AppHandle, name: String, args: serde_json::Value) -> bool {
    let args = display_args(&args);
    emit(
        app,
        AgentEvent::ToolCall(ToolCall {
            app: tool_app(&name),
            name,
            args,
            requires_confirmation: true,
        }),
    );
    await_confirmation(app).await
}

/// Flattens a tool's argument object into displayable string fields.
///
/// Strings pass through unquoted; anything else keeps its JSON form so a
/// number or nested object is still readable.
fn display_args(args: &serde_json::Value) -> std::collections::BTreeMap<String, String> {
    args.as_object()
        .map(|map| {
            map.iter()
                .map(|(key, value)| {
                    let text = match value {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (key.clone(), text)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Maps a loop event to the panel's AG-UI event vocabulary.
fn event_of(event: aloud_agent::LoopEvent) -> AgentEvent {
    match event {
        aloud_agent::LoopEvent::ToolCallStart { name, args } => AgentEvent::ToolCall(ToolCall {
            app: tool_app(&name),
            name,
            args: display_args(&args),
            // The gate is enforced in the loop; the panel just shows the call.
            requires_confirmation: false,
        }),
        aloud_agent::LoopEvent::ToolCallResult { name, success } => {
            AgentEvent::ToolResult(ToolResult {
                success,
                message: if success {
                    format!("{name} done")
                } else {
                    format!("{name} failed")
                },
            })
        }
        aloud_agent::LoopEvent::RateLimited { seconds } => {
            AgentEvent::Status(format!("Rate limited — retrying in {seconds}s…"))
        }
    }
}

/// The toolkit a Composio tool slug belongs to, for display.
fn tool_app(slug: &str) -> String {
    if slug.starts_with("GMAIL_") {
        "gmail".to_string()
    } else {
        "github".to_string()
    }
}

/// Blocks until the panel approves or rejects. A dropped sender (panel closed,
/// new command started) reads as a rejection — the safe default for a write.
///
/// Times out rather than waiting forever. A gate that can hang is a gate that
/// wedges the whole agent: a panel bug once left a second write blocked on a
/// button that was never drawn, with no way out but quitting the app. Expiring
/// as "declined" fails in the safe direction.
async fn await_confirmation(app: &AppHandle) -> bool {
    let (tx, rx) = oneshot::channel();
    if let Some(state) = app.try_state::<PendingConfirmation>() {
        *state.0.lock().unwrap() = Some(tx);
    } else {
        tracing::error!("PendingConfirmation state missing");
        return false;
    }
    match tokio::time::timeout(CONFIRM_TIMEOUT, rx).await {
        Ok(answer) => answer.unwrap_or(false),
        Err(_) => {
            tracing::warn!("confirmation timed out; treating as declined");
            false
        }
    }
}

/// Resolves the pending confirmation. Called by the panel's Yes/No buttons.
#[tauri::command]
#[specta::specta]
pub fn confirm_agent(app: AppHandle, approved: bool) {
    if let Some(state) = app.try_state::<PendingConfirmation>() {
        if let Some(tx) = state.0.lock().unwrap().take() {
            // The receiver is gone only if the flow was already abandoned;
            // ignore the send error in that case.
            let _ = tx.send(approved);
        }
    }
    if !approved {
        hide(&app);
    }
}

fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        tracing::warn!("agent window missing");
        return;
    };
    // Same top-centre drop as the ask panel; the frontend grows it downward.
    if let Err(e) = crate::overlay::position_top_center(&window, 12.0) {
        tracing::warn!(%e, "could not position agent panel");
    }
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

/// Lets the panel dismiss itself on Escape or after showing a result.
#[tauri::command]
#[specta::specta]
pub fn close_agent(app: AppHandle) {
    // Closing the panel abandons any pending confirmation, which reads as a
    // rejection — the write never fires.
    if let Some(state) = app.try_state::<PendingConfirmation>() {
        state.0.lock().unwrap().take();
    }
    hide(&app);
}
