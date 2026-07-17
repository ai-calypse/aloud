//! Speech-to-text with two interchangeable engines.
//!
//! The Swift app used cloud Whisper (Groq) with `SFSpeechRecognizer` as a
//! macOS-only offline fallback. Here local whisper.cpp replaces that fallback,
//! so offline dictation works on Windows too, and either engine can be the
//! default — the "bring your own model" shape anarlog uses.

pub mod cloud;
pub mod download;
pub mod hallucination;
pub mod local;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One engine's worth of transcription. Callers pick an implementation at
/// runtime from settings and never branch on it again.
#[async_trait]
pub trait Transcriber: Send + Sync {
    /// Transcribes a 16 kHz mono PCM16 WAV.
    ///
    /// Returns an empty string when the audio was silence — that is a normal
    /// outcome, not an error.
    async fn transcribe(&self, wav_path: &Path) -> Result<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Engine {
    /// On-device whisper.cpp. No network, no API key.
    Local,
    /// An OpenAI-compatible endpoint (Groq by default).
    Cloud,
}
