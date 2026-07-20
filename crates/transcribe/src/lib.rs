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

/// Whether a failed cloud transcription is worth retrying on-device.
///
/// **Deliberately not a network monitor.** Reachability is a second source of
/// truth that can disagree with the request that just failed: it reports
/// "online" behind a captive portal, and "offline" for the half-second after a
/// Wi-Fi hop the request would have survived. The error the request returned is
/// the only first-hand account of what happened, so that is what is read.
///
/// Note the contrast with `aloud_llm`'s `complete_with_fallback`, which retries
/// on a rate limit *only*, because "anything else fails identically on the
/// fallback" — the fallback there is another model at the same provider. That
/// reasoning does not hold here: the fallback is a different engine on this
/// machine, so the failures worth retrying are exactly the ones the network or
/// the provider caused. A 401 (rejected key) and a 5xx (their fault) both clear
/// on-device; a 400 is a request this crate built wrong and would fail again,
/// so it still surfaces.
pub fn is_recoverable_locally(error: &anyhow::Error) -> bool {
    if let Some(http) = error.downcast_ref::<cloud::HttpError>() {
        return http.status == 401 || http.status >= 500;
    }
    // DNS, refused connection, TLS, timeout — reqwest is the only thing in this
    // crate that produces one, and every one of them is transport.
    error
        .chain()
        .any(|e| e.downcast_ref::<reqwest::Error>().is_some())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Engine {
    /// On-device whisper.cpp. No network, no API key.
    Local,
    /// An OpenAI-compatible endpoint (Groq by default).
    Cloud,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The classification is the whole of the offline fallback: get it wrong in
    /// one direction and a genuinely broken request is retried on-device for
    /// nothing, in the other and pulling the network mid-dictation loses the
    /// text.
    #[test]
    fn only_the_provider_s_own_failures_fall_back() {
        let recoverable = |status, body: &str| {
            is_recoverable_locally(
                &anyhow::Error::from(cloud::HttpError {
                    status,
                    body: body.into(),
                })
                .context("transcribing"),
            )
        };
        assert!(recoverable(401, "invalid api key"), "a rejected key");
        assert!(recoverable(500, ""), "their fault");
        assert!(recoverable(503, "upstream unavailable"));
        assert!(!recoverable(400, "bad multipart"), "our request, our bug");
        assert!(!recoverable(404, "no such model"));
        // A 429 is the provider working correctly and saying "wait", not an
        // outage. The task list draws the line at 401/5xx and transport, and
        // widening it here without the same evidence is how the two drift.
        assert!(!recoverable(429, "slow down"));
    }

    #[tokio::test]
    async fn a_refused_connection_falls_back() {
        // A real transport error rather than a hand-built one: the whole point
        // is that this classifies what reqwest actually returns.
        let error = reqwest::get("http://127.0.0.1:1/audio/transcriptions")
            .await
            .expect_err("port 1 refuses connections");
        assert!(is_recoverable_locally(
            &anyhow::Error::from(error).context("transcription request failed")
        ));
    }
}
