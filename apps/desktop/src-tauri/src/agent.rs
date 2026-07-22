//! Agent mode: run a spoken command across apps.
//!
//!   transcribe → parse intent → (confirm if it writes) → dispatch → report
//!
//! An action never fires without explicit approval: reads run straight through,
//! writes surface a confirmation and wait for a yes. Drawn in the overlay's
//! drop-down panel, like Ask, but this one collects a decision — hence the
//! one-shot channel the approve/reject commands resolve.

use aloud_store::Settings;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::sync::oneshot;
use tracing::Instrument;

/// The window these events go to — the overlay's, not one of Agent's own.
const LABEL: &str = "overlay";
/// How long a write waits for a yes before giving up and declining.
const CONFIRM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// The pending confirmation's reply channel, so approve/reject can wake the
/// awaiting dispatch. Only one is ever in flight; a new command replaces it.
#[derive(Default)]
pub struct PendingConfirmation(Mutex<Option<oneshot::Sender<bool>>>);

/// Streamed to the agent panel. Names follow the AG-UI protocol's tool-call
/// lifecycle (https://docs.ag-ui.com) so the shape maps onto AI Elements' tool
/// renderer — the vocabulary only, not the wire protocol.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
#[tauri_specta(event_name = "agent")]
pub enum AgentEvent {
    /// The transcript, echoed so the panel shows what it heard.
    RunStarted(String),
    /// The batch a round intends to run, sent before the first call so a run
    /// about to send mail can still be read. Only when a round holds >1 call.
    Planned(Vec<PlannedCall>),
    /// A parsed action awaiting dispatch. A write pauses here for confirmation.
    ToolCall(ToolCall),
    ToolResult(ToolResult),
    /// A transient status line (e.g. "rate limited, retrying in 30s") shown
    /// while the run is otherwise silent; cleared by the next event.
    Status(String),
    RunFinished(String),
    RunError(String),
}

/// A tool the run intends to call. No arguments: they arrive with the call
/// itself, which is where it stops being a plan and starts being an action.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlannedCall {
    pub name: String,
    pub app: String,
}

/// A tool call for display. Kept separate from `ParsedAction` so the UI
/// contract does not leak parser internals.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    /// e.g. "github.create_issue".
    pub name: String,
    /// The target app, e.g. "github".
    pub app: String,
    /// Args as strings, not `serde_json::Value`: specta refuses to export its
    /// i64-backed number type. Non-string values keep their JSON form.
    pub args: std::collections::BTreeMap<String, String>,
    /// Whether this call pauses for confirmation before it runs (every write).
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub success: bool,
    pub message: String,
}

fn emit(app: &AppHandle, event: AgentEvent) {
    if let Err(e) = event.emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send to agent panel");
    }
}

/// Runs the full agent flow for a transcript. Called by the pipeline.
pub async fn run(app: &AppHandle, transcript: String, settings: &Settings) -> Result<()> {
    crate::overlay::show_panel(app);
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

// Gemini, not Groq: a multi-round tool loop makes several chat calls in a row
// and Groq's tokens-per-minute limit throttles it mid-command. The endpoint is
// OpenAI-compatible, so the same LlmClient works unchanged.
const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
const GEMINI_MODEL: &str = "gemini-3.1-flash-lite";

#[tracing::instrument(
    name = "agent_run",
    skip_all,
    fields(tool_count = tracing::field::Empty)
)]
async fn execute(app: &AppHandle, transcript: &str, settings: &Settings) -> Result<String> {
    if settings.gemini_api_key.trim().is_empty() {
        bail!("Agent mode is unavailable — no Gemini credential is configured");
    }
    if settings.composio_api_key.trim().is_empty() {
        bail!("Agent mode is unavailable — no Composio credential is configured");
    }

    let llm = aloud_llm::LlmClient::new(&settings.gemini_api_key, Some(GEMINI_BASE_URL))?;
    let composio = aloud_agent::ComposioClient::new(&settings.composio_api_key)?;

    // A Composio round-trip on every command, before the model is even asked —
    // invisible in any per-tool timing, so it gets its own span.
    let tools = composio
        .available_tools(transcript)
        .instrument(tracing::info_span!(
            "tool_discovery",
            count = tracing::field::Empty
        ))
        .await?;
    tracing::Span::current().record("tool_count", tools.len());
    if tools.is_empty() {
        bail!("No app is connected in Composio yet");
    }

    aloud_agent::run_tool_loop(
        &llm,
        GEMINI_MODEL,
        &composio,
        transcript,
        &tools,
        |event| emit(app, event_of(event)),
        // The safety gate. The offered tools carry their toolkit, so it names
        // the real app rather than deriving one from the slug.
        |name, args| {
            let toolkit = tools
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.toolkit.clone())
                .unwrap_or_else(|| tool_app(&name));
            confirm_write(app, name, toolkit, args)
        },
    )
    .await
}

/// Surfaces a write tool for confirmation and awaits the panel's decision. Takes
/// owned args so the returned future doesn't borrow the loop's transient data.
async fn confirm_write(
    app: &AppHandle,
    name: String,
    toolkit: String,
    args: serde_json::Value,
) -> bool {
    let args = display_args(&args);
    emit(
        app,
        AgentEvent::ToolCall(ToolCall {
            app: toolkit,
            name,
            args,
            requires_confirmation: true,
        }),
    );
    await_confirmation(app).await
}

/// Flattens a tool's argument object into displayable string fields.
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

fn event_of(event: aloud_agent::LoopEvent) -> AgentEvent {
    match event {
        aloud_agent::LoopEvent::Planned { names } => AgentEvent::Planned(
            names
                .into_iter()
                .map(|name| PlannedCall {
                    app: tool_app(&name),
                    name,
                })
                .collect(),
        ),
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
    aloud_agent::toolkit_of_slug(slug)
}

/// Blocks until the panel approves or rejects. A dropped sender (panel closed,
/// new command started) or a timeout reads as a rejection — a gate that can
/// hang wedges the whole agent, and declining fails in the safe direction.
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
            // A gone receiver means the flow was already abandoned.
            let _ = tx.send(approved);
        }
    }
    if !approved {
        hide(&app);
    }
}

pub fn hide(app: &AppHandle) {
    crate::overlay::end_panel(app);
}

/// Lets the panel dismiss itself on Escape or after showing a result.
#[tauri::command]
#[specta::specta]
pub fn close_agent(app: AppHandle) {
    // Abandoning a pending confirmation reads as a rejection; the write never
    // fires.
    if let Some(state) = app.try_state::<PendingConfirmation>() {
        state.0.lock().unwrap().take();
    }
    hide(&app);
}
