//! Claude plan-quota usage — how much of the rolling limits is spent.
//!
//! The path mirrors notchi's `ClaudeUsageService`: read the Claude Code OAuth
//! token from the login Keychain, then `GET /api/oauth/usage` with it. The
//! response carries a 5-hour window and weekly windows, each a utilization
//! percentage and a reset time.
//!
//! MVP of that approach — no silent token refresh and no `/v1/messages` header
//! fallback (notchi's two resilience layers). An expired token surfaces as
//! [`QuotaError::Unauthorized`] rather than being refreshed; Claude Code itself
//! refreshes it in the normal course of use.
//!
//! macOS only: the token lives in the macOS Keychain. Other platforms return
//! [`QuotaError::Unsupported`].

use std::time::Duration;

use serde::Deserialize;

mod keychain;

/// The Anthropic OAuth usage endpoint — same one Claude Code's own UI uses.
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// The beta header the endpoint requires.
const OAUTH_BETA: &str = "oauth-2025-04-20";

#[derive(Debug, thiserror::Error)]
pub enum QuotaError {
    #[error("no Claude Code token in the keychain — is Claude Code signed in?")]
    NoToken,
    #[error("the Claude Code token was rejected (likely expired)")]
    Unauthorized,
    #[error("usage request failed: {0}")]
    Request(String),
    #[error("keychain read is macOS-only")]
    Unsupported,
}

/// One rolling limit window: how full it is and when it resets.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    /// 0–100.
    pub percent: f64,
    /// ISO-8601 reset time, verbatim from the API. Formatting the countdown is
    /// the UI's job — it ticks without a new fetch.
    pub resets_at: Option<String>,
}

/// The parsed usage: the windows a dashboard shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Quota {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub seven_day_opus: Option<Window>,
    /// Extra (pay-as-you-go over plan) utilization %, when the account has it on.
    pub extra_usage_percent: Option<f64>,
}

/// Read the token from the Keychain and fetch current usage. One shot — the
/// caller polls.
pub async fn fetch() -> Result<Quota, QuotaError> {
    let token = keychain::claude_token()?;
    fetch_with_token(&token).await
}

/// One pooled client, built once. A client-per-request drops the connection
/// pool and re-does the TCP+TLS handshake every 60s poll; sharing keeps the
/// keep-alive to `api.anthropic.com` warm. `reqwest::Client` is `Arc` inside,
/// cheap to reuse across calls.
fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default()
    })
}

/// Fetch usage with an explicit token — the testable seam, and how a future
/// env-var source would feed in.
pub async fn fetch_with_token(token: &str) -> Result<Quota, QuotaError> {
    let response = client()
        .get(USAGE_URL)
        .bearer_auth(token)
        .header("Accept", "application/json")
        .header("anthropic-beta", OAUTH_BETA)
        // A recognizable UA; the endpoint is Claude Code's own.
        .header("User-Agent", "aloud/0.1 (quota)")
        .send()
        .await
        .map_err(|e| QuotaError::Request(e.to_string()))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED
        || response.status() == reqwest::StatusCode::FORBIDDEN
    {
        // The token was rejected — likely rotated by Claude Code. Drop the cache
        // so the next fetch re-reads it (one prompt, not a loop).
        keychain::poison();
        return Err(QuotaError::Unauthorized);
    }
    if !response.status().is_success() {
        return Err(QuotaError::Request(format!("HTTP {}", response.status())));
    }

    let body = response
        .text()
        .await
        .map_err(|e| QuotaError::Request(e.to_string()))?;
    parse(&body).map_err(|e| QuotaError::Request(e.to_string()))
}

/// Parse the usage JSON. Split out so the shape is testable without the network.
pub fn parse(body: &str) -> anyhow::Result<Quota> {
    let raw: RawUsage = serde_json::from_str(body)?;
    Ok(Quota {
        five_hour: raw.five_hour.map(Into::into),
        seven_day: raw.seven_day.map(Into::into),
        seven_day_opus: raw.seven_day_opus.map(Into::into),
        extra_usage_percent: raw.extra_usage.and_then(|e| e.utilization),
    })
}

#[derive(Deserialize)]
struct RawUsage {
    five_hour: Option<RawPeriod>,
    seven_day: Option<RawPeriod>,
    seven_day_opus: Option<RawPeriod>,
    extra_usage: Option<RawExtra>,
}

#[derive(Deserialize)]
struct RawPeriod {
    #[serde(default)]
    utilization: f64,
    #[serde(default)]
    resets_at: Option<String>,
}

#[derive(Deserialize)]
struct RawExtra {
    utilization: Option<f64>,
}

impl From<RawPeriod> for Window {
    fn from(p: RawPeriod) -> Self {
        Window {
            percent: p.utilization,
            resets_at: p.resets_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_and_extra() {
        let body = r#"{
            "five_hour": {"utilization": 42.5, "resets_at": "2026-07-24T18:00:00Z"},
            "seven_day": {"utilization": 12.0, "resets_at": "2026-07-30T00:00:00Z"},
            "seven_day_opus": {"utilization": 88.0},
            "extra_usage": {"is_enabled": true, "utilization": 15.0}
        }"#;
        let q = parse(body).unwrap();
        assert_eq!(q.five_hour.as_ref().unwrap().percent, 42.5);
        assert_eq!(
            q.five_hour.as_ref().unwrap().resets_at.as_deref(),
            Some("2026-07-24T18:00:00Z")
        );
        assert_eq!(q.seven_day_opus.unwrap().percent, 88.0);
        assert_eq!(q.extra_usage_percent, Some(15.0));
    }

    #[test]
    fn tolerates_missing_windows() {
        let q = parse(r#"{"five_hour": {"utilization": 5.0}}"#).unwrap();
        assert!(q.seven_day.is_none());
        assert!(q.extra_usage_percent.is_none());
        assert_eq!(q.five_hour.unwrap().resets_at, None);
    }
}
