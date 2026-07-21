//! Transcription via any OpenAI-compatible `/audio/transcriptions` endpoint.
//!
//! Ported from `TranscriptionService.swift`. Defaults to Groq, but the base
//! URL is configurable so OpenAI or a self-hosted endpoint work unchanged.

use crate::{hallucination, Transcriber};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::{path::Path, time::Duration};

pub const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";
pub const DEFAULT_MODEL: &str = "whisper-large-v3";
const TIMEOUT: Duration = Duration::from_secs(20);

/// Models that return segment metadata, which the hallucination filter needs.
/// The gpt-4o-transcribe family only supports plain JSON.
const VERBOSE_JSON_MODELS: &[&str] = &["whisper-1", "whisper-large-v3", "whisper-large-v3-turbo"];

pub struct CloudTranscriber {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    language: Option<String>,
}

/// One connection pool for the process.
///
/// A `CloudTranscriber` is built on every dictation — the pipeline caches only
/// the local engine, deliberately, because caching this one would freeze the
/// key and base URL until restart. That is fine, but a fresh
/// `reqwest::Client` per transcriber also meant a fresh *pool*, so each
/// dictation paid a full TCP+TLS handshake: 85-130ms measured against Groq.
/// `reqwest::Client` is an `Arc` inside, so this keeps the per-dictation
/// construction and shares the connections.
static HTTP: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

fn shared_http() -> Result<reqwest::Client> {
    // The timeout moves to the request: this pool is per process, and 20s is
    // right for an audio upload but not for everything that might share it.
    match HTTP.get() {
        Some(client) => Ok(client.clone()),
        None => {
            let client = reqwest::Client::builder()
                .build()
                .context("building http client")?;
            Ok(HTTP.get_or_init(|| client).clone())
        }
    }
}

/// A non-2xx from the provider. Typed rather than a formatted `bail!` so a
/// caller can tell an expired credential or a provider outage from a request
/// this crate got wrong — see [`crate::is_recoverable_locally`].
#[derive(Debug, thiserror::Error)]
#[error("transcription failed ({status}): {body}")]
pub struct HttpError {
    pub status: u16,
    pub body: String,
}

#[derive(Deserialize)]
struct Response {
    text: String,
    /// Absent unless the model supports verbose_json.
    #[serde(default)]
    segments: Vec<Segment>,
}

#[derive(Deserialize)]
struct Segment {
    #[serde(default)]
    no_speech_prob: Option<f64>,
}

impl CloudTranscriber {
    pub fn new(
        api_key: impl Into<String>,
        base_url: Option<&str>,
        model: Option<&str>,
        language: Option<&str>,
    ) -> Result<Self> {
        let model = model.map(str::trim).filter(|m| !m.is_empty());
        Ok(Self {
            client: shared_http()?,
            base_url: normalize_base_url(base_url.unwrap_or(DEFAULT_BASE_URL)),
            api_key: api_key.into(),
            model: model.unwrap_or(DEFAULT_MODEL).to_string(),
            language: language
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string),
        })
    }

    fn response_format(&self) -> &'static str {
        if VERBOSE_JSON_MODELS.contains(&self.model.to_lowercase().as_str()) {
            "verbose_json"
        } else {
            "json"
        }
    }
}

#[async_trait]
impl Transcriber for CloudTranscriber {
    async fn transcribe(&self, wav_path: &Path) -> Result<String> {
        let audio = tokio::fs::read(wav_path)
            .await
            .with_context(|| format!("reading {}", wav_path.display()))?;

        let mut form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(audio)
                    .file_name("audio.wav")
                    .mime_str("audio/wav")?,
            )
            .text("model", self.model.clone())
            .text("response_format", self.response_format());

        if let Some(language) = &self.language {
            form = form.text("language", language.clone());
        }

        let response = self
            .client
            .post(format!("{}/audio/transcriptions", self.base_url))
            // Per request: the client is process-wide now.
            .timeout(TIMEOUT)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .context("transcription request failed")?;

        let status = response.status();
        let body = response.text().await.context("reading response")?;
        if !status.is_success() {
            return Err(HttpError {
                status: status.as_u16(),
                body,
            }
            .into());
        }

        let parsed: Response =
            serde_json::from_str(&body).with_context(|| format!("unexpected response: {body}"))?;

        let no_speech_prob = parsed.segments.first().and_then(|s| s.no_speech_prob);
        if hallucination::is_hallucination(&parsed.text, no_speech_prob) {
            tracing::info!(text = %parsed.text, "dropped hallucinated transcript");
            return Ok(String::new());
        }
        Ok(parsed.text)
    }
}

/// Trims trailing slashes so joining paths cannot produce `//`.
fn normalize_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbose_json_only_for_models_that_support_it() {
        let whisper = CloudTranscriber::new("k", None, Some("whisper-large-v3"), None).unwrap();
        assert_eq!(whisper.response_format(), "verbose_json");

        let gpt4o = CloudTranscriber::new("k", None, Some("gpt-4o-transcribe"), None).unwrap();
        assert_eq!(gpt4o.response_format(), "json");
    }

    #[test]
    fn model_matching_ignores_case() {
        let t = CloudTranscriber::new("k", None, Some("Whisper-Large-V3"), None).unwrap();
        assert_eq!(t.response_format(), "verbose_json");
    }

    #[test]
    fn blank_model_and_language_fall_back_to_defaults() {
        let t = CloudTranscriber::new("k", None, Some("  "), Some("  ")).unwrap();
        assert_eq!(t.model, DEFAULT_MODEL);
        assert_eq!(t.language, None);
    }

    #[test]
    fn base_url_loses_trailing_slash() {
        assert_eq!(
            normalize_base_url("https://api.groq.com/openai/v1/"),
            "https://api.groq.com/openai/v1"
        );
    }
}
