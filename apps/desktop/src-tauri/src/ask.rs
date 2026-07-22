//! Ask: streams a markdown answer into the overlay's drop-down panel.
//!
//! There is no Ask window — the answer grows into the same panel that said
//! "Listening…". `overlay::show_answer` owns the hand-off; this module only
//! produces the text.

use aloud_store::Settings;
use anyhow::{bail, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;

/// The window these events go to — the overlay's, not one of Ask's own.
const LABEL: &str = "overlay";

/// Streamed to the panel as the answer arrives.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
#[tauri_specta(event_name = "ask")]
pub enum AskEvent {
    /// The question, echoed so the panel can show what it heard.
    Question(String),
    Delta(String),
    Done,
    Failed(String),
}

fn emit(app: &AppHandle, event: AskEvent) {
    if let Err(e) = event.emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send to ask panel");
    }
}

/// Shows the panel and streams an answer to `question` into it.
pub async fn answer(app: &AppHandle, question: String, settings: &Settings) -> Result<()> {
    crate::overlay::show_panel(app);
    emit(app, AskEvent::Question(question.clone()));

    match stream_answer(app, &question, settings).await {
        Ok(()) => emit(app, AskEvent::Done),
        Err(e) => {
            tracing::error!(%e, "ask failed");
            emit(app, AskEvent::Failed(format!("{e:#}")));
        }
    }
    Ok(())
}

async fn stream_answer(app: &AppHandle, question: &str, settings: &Settings) -> Result<()> {
    if settings.groq_api_key.trim().is_empty() {
        bail!("Ask is unavailable — no credential is configured");
    }

    let client = aloud_llm::LlmClient::new(&settings.groq_api_key, Some(&settings.base_url))?;
    let messages = [
        aloud_llm::Message::system(aloud_llm::ask::SYSTEM_PROMPT),
        aloud_llm::Message::user(question),
    ];

    let stream = client
        .stream(&settings.post_processing_model, &messages, Some(0.3))
        .await?;
    // `next()` needs the stream pinned; it is an opaque `impl Stream`.
    let mut stream = std::pin::pin!(stream);

    while let Some(delta) = stream.next().await {
        emit(app, AskEvent::Delta(delta?));
    }
    Ok(())
}

/// Answers a question the user typed into the panel.
#[tauri::command]
#[specta::specta]
pub async fn ask_text(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    question: String,
) -> Result<(), String> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Ok(());
    }
    // Cloned rather than held: the guard is not Send across the await below.
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    answer(&app, question, &settings)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Lets the panel dismiss itself on Escape or the close button.
#[tauri::command]
#[specta::specta]
pub fn close_ask(app: AppHandle) {
    crate::overlay::end_panel(&app);
}
