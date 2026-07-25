//! The plan-quota dashboard: Claude's rolling usage limits, for the webview.
//!
//! Thin wiring over `aloud_quota`, which does the Keychain read and the usage
//! fetch. This file only flattens the crate types to specta and exposes the
//! command; the logic and its failure modes live in the crate.

use serde::{Deserialize, Serialize};

/// One rolling limit window, flattened for the webview.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    /// 0–100.
    pub percent: f64,
    /// ISO-8601 reset time; the webview renders the countdown so it ticks
    /// without a refetch.
    pub resets_at: Option<String>,
}

/// The whole quota, as the dashboard shows it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub five_hour: Option<QuotaWindow>,
    pub seven_day: Option<QuotaWindow>,
    pub seven_day_opus: Option<QuotaWindow>,
    pub extra_usage_percent: Option<f64>,
}

impl From<aloud_quota::Window> for QuotaWindow {
    fn from(w: aloud_quota::Window) -> Self {
        QuotaWindow {
            percent: w.percent,
            resets_at: w.resets_at,
        }
    }
}

impl From<aloud_quota::Quota> for Quota {
    fn from(q: aloud_quota::Quota) -> Self {
        Quota {
            five_hour: q.five_hour.map(Into::into),
            seven_day: q.seven_day.map(Into::into),
            seven_day_opus: q.seven_day_opus.map(Into::into),
            extra_usage_percent: q.extra_usage_percent,
        }
    }
}

/// Read the Claude Code token from the Keychain and fetch current usage.
///
/// The webview polls this. The first Keychain read may raise a macOS access
/// prompt — that's the OS's, and expected. An `Err` string is shown as-is in
/// the dashboard's empty state (e.g. "not signed in", "token expired").
#[tauri::command]
#[specta::specta]
pub async fn agent_quota() -> Result<Quota, String> {
    aloud_quota::fetch()
        .await
        .map(Quota::from)
        .map_err(|e| e.to_string())
}
