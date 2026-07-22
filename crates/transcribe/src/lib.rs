//! Speech-to-text with two interchangeable engines: local whisper.cpp and any
//! OpenAI-compatible cloud endpoint. Either can be the default.

pub mod cloud;
pub mod download;
pub mod hallucination;
pub mod local;
pub mod preview;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One engine's worth of transcription; callers pick an implementation from
/// settings and never branch on it again.
#[async_trait]
pub trait Transcriber: Send + Sync {
    /// Transcribes a 16 kHz mono PCM16 WAV. Silence returns an empty string,
    /// which is a normal outcome, not an error.
    async fn transcribe(&self, wav_path: &Path) -> Result<String>;
}

/// Whether a failed cloud transcription is worth retrying on-device.
///
/// **Deliberately not a network monitor**: reachability reports "online" behind
/// a captive portal and "offline" just after a Wi-Fi hop the request survived.
/// The error the request returned is the only first-hand account.
///
/// The fallback is a different engine on this machine, so what is worth
/// retrying is what the network or the provider caused: 401 (rejected key) and
/// 5xx clear on-device, a 400 is our own bad request and still surfaces.
pub fn is_recoverable_locally(error: &anyhow::Error) -> bool {
    if let Some(http) = error.downcast_ref::<cloud::HttpError>() {
        return http.status == 401 || http.status >= 500;
    }
    // DNS, refused connection, TLS, timeout — all transport.
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

    /// Wrong one way, a broken request is retried on-device for nothing; wrong
    /// the other, pulling the network mid-dictation loses the text.
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
        // A 429 is the provider working correctly and saying "wait".
        assert!(!recoverable(429, "slow down"));
    }

    #[tokio::test]
    async fn a_refused_connection_falls_back() {
        // A real transport error: the point is classifying what reqwest returns.
        let error = reqwest::get("http://127.0.0.1:1/audio/transcriptions")
            .await
            .expect_err("port 1 refuses connections");
        assert!(is_recoverable_locally(
            &anyhow::Error::from(error).context("transcription request failed")
        ));
    }
}
