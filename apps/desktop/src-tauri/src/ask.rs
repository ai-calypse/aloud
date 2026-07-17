//! The Ask answer panel: a floating window that streams a markdown answer.
//!
//! Unlike the recording pill this one takes focus — the user reads and scrolls
//! it, and Escape dismisses it.

use aloud_store::Settings;
use anyhow::{bail, Result};
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

const LABEL: &str = "ask";

/// Streamed to the panel as the answer arrives.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum AskEvent {
    /// The question, echoed so the panel can show what it heard.
    Question(String),
    /// One incremental chunk of the answer.
    Delta(String),
    Done,
    Failed(String),
}

fn emit(app: &AppHandle, event: AskEvent) {
    if let Err(e) = app.emit_to(LABEL, "ask", event) {
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

fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        tracing::warn!("ask window missing");
        return;
    };
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
