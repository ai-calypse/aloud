//! The Ask answer panel: a floating window that streams a markdown answer.
//!
//! Unlike the recording pill this one takes focus — the user reads and scrolls
//! it, and Escape dismisses it.

use aloud_store::Settings;
use anyhow::{bail, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

const LABEL: &str = "ask";

/// Streamed to the panel as the answer arrives.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
#[tauri_specta(event_name = "ask")]
pub enum AskEvent {
    /// The question, echoed so the panel can show what it heard.
    Question(String),
    /// One incremental chunk of the answer.
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
    show(app);
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
        bail!("Ask needs an API key — add one in Settings");
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

/// Gap between the screen top and the panel, per the Swift answer panel.
const TOP_GAP: f64 = 12.0;

fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        tracing::warn!("ask window missing");
        return;
    };
    // Anchor the top edge; the frontend grows the window downward to fit.
    if let Err(e) = crate::overlay::position_top_center(&window, TOP_GAP) {
        tracing::warn!(%e, "could not position ask panel");
    }
    let _ = window.show();
    // Unlike the pill, this window is read and scrolled, so it takes focus.
    let _ = window.set_focus();
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

/// Lets the panel dismiss itself on Escape or a click outside.
#[tauri::command]
#[specta::specta]
pub fn close_ask(app: AppHandle) {
    hide(&app);
}
