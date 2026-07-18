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

use aloud_agent::{Dispatcher, GitHubConnector, ParsedAction};
use aloud_store::Settings;
use anyhow::{bail, Result};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

const LABEL: &str = "agent";

/// The one connector that ships today. A registry is overkill for one entry;
/// when the second connector lands this becomes a builder that reads settings
/// to decide which to register.
fn dispatcher() -> Dispatcher {
    let mut d = Dispatcher::new();
    d.register(Arc::new(GitHubConnector::new()));
    d
}

/// Holds the pending confirmation's reply channel, so the frontend's
/// approve/reject command can wake the awaiting dispatch. Only one confirmation
/// is ever in flight — a new agent command replaces any stale pending one.
#[derive(Default)]
pub struct PendingConfirmation(Mutex<Option<oneshot::Sender<bool>>>);

/// Streamed to the agent panel.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum AgentEvent {
    /// The transcript, echoed so the panel shows what it heard.
    Heard(String),
    /// A parsed write action awaiting confirmation.
    Confirm(ActionSummary),
    /// Plain-language progress ("Working…", "Opened issue: …").
    Status(String),
    Done(String),
    Failed(String),
}

/// The parts of a [`ParsedAction`] the confirmation panel shows. Kept separate
/// from `ParsedAction` so the UI contract does not leak parser internals.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActionSummary {
    pub action: String,
    pub app: String,
    /// Human-ordered "field: value" lines, e.g. ["repo: o/r", "title: Bug"].
    pub details: Vec<String>,
}

impl ActionSummary {
    fn of(action: &ParsedAction) -> Self {
        Self {
            action: action.action.clone(),
            app: action.app.clone(),
            details: action
                .entities
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect(),
        }
    }
}

fn emit(app: &AppHandle, event: AgentEvent) {
    if let Err(e) = app.emit_to(LABEL, "agent", event) {
        tracing::warn!(%e, "could not send to agent panel");
    }
}

/// Runs the full agent flow for a transcript. Called by the pipeline.
pub async fn run(app: &AppHandle, transcript: String, settings: &Settings) -> Result<()> {
    show(app);
    emit(app, AgentEvent::Heard(transcript.clone()));

    match execute(app, &transcript, settings).await {
        Ok(message) => emit(app, AgentEvent::Done(message)),
        Err(e) => {
            tracing::error!(%e, "agent failed");
            emit(app, AgentEvent::Failed(format!("{e:#}")));
        }
    }
    Ok(())
}

// Agent intent parsing runs on Cerebras, not Groq: the tool-call parse is a
// heavier request and Groq's tokens-per-minute limit throttles it. Cerebras is
// OpenAI-compatible, so the same LlmClient works against its endpoint.
const CEREBRAS_BASE_URL: &str = "https://api.cerebras.ai/v1";
const CEREBRAS_MODEL: &str = "gpt-oss-120b";

async fn execute(app: &AppHandle, transcript: &str, settings: &Settings) -> Result<String> {
    if settings.cerebras_api_key.trim().is_empty() {
        bail!("Agent mode needs a Cerebras API key to understand commands — add one in Settings");
    }

    let client =
        aloud_llm::LlmClient::new(&settings.cerebras_api_key, Some(CEREBRAS_BASE_URL))?;
    let action = aloud_agent::parse_intent(&client, CEREBRAS_MODEL, transcript)
        .await?
        .ok_or_else(|| anyhow::anyhow!("I didn't recognize that as a command I can run"))?;

    // A write waits for an explicit yes; a read runs straight through. This is
    // the one non-negotiable safety gate — never dispatch a write unconfirmed.
    if action.requires_confirmation {
        emit(app, AgentEvent::Confirm(ActionSummary::of(&action)));
        if !await_confirmation(app).await {
            return Ok("Cancelled.".to_string());
        }
    }

    emit(app, AgentEvent::Status("Working…".to_string()));
    let result = dispatcher().dispatch(&action).await;
    if result.success {
        Ok(result.message)
    } else {
        bail!("{}", result.message)
    }
}

/// Blocks until the panel approves or rejects. A dropped sender (panel closed,
/// new command started) reads as a rejection — the safe default for a write.
async fn await_confirmation(app: &AppHandle) -> bool {
    let (tx, rx) = oneshot::channel();
    if let Some(state) = app.try_state::<PendingConfirmation>() {
        *state.0.lock().unwrap() = Some(tx);
    } else {
        tracing::error!("PendingConfirmation state missing");
        return false;
    }
    rx.await.unwrap_or(false)
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
