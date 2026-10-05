//! The plan-quota dashboard: Claude's rolling usage limits, for the webview.
//!
//! Thin wiring over `aloud_quota`, which does the Keychain read and the usage
//! fetch. This file only flattens the crate types to specta and exposes the
//! command; the logic and its failure modes live in the crate.

use aloud_store::quota_history::{self, QuotaSample};
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

/// The extra-usage credit pool, amounts in minor units (cents) of `currency`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct QuotaExtra {
    pub enabled: bool,
    pub monthly_limit: Option<f64>,
    pub used_credits: Option<f64>,
    pub currency: Option<String>,
    pub utilization: Option<f64>,
}

/// The whole quota, as the dashboard shows it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub five_hour: Option<QuotaWindow>,
    pub seven_day: Option<QuotaWindow>,
    pub seven_day_opus: Option<QuotaWindow>,
    pub seven_day_sonnet: Option<QuotaWindow>,
    pub seven_day_oauth_apps: Option<QuotaWindow>,
    pub seven_day_cowork: Option<QuotaWindow>,
    pub seven_day_omelette: Option<QuotaWindow>,
    pub extra_usage_percent: Option<f64>,
    pub extra: Option<QuotaExtra>,
    /// The app's own readings of the last week, oldest first, this one last.
    pub history: Vec<QuotaSample>,
}

impl From<aloud_quota::Window> for QuotaWindow {
    fn from(w: aloud_quota::Window) -> Self {
        QuotaWindow {
            percent: w.percent,
            resets_at: w.resets_at,
        }
    }
}

impl From<aloud_quota::Extra> for QuotaExtra {
    fn from(e: aloud_quota::Extra) -> Self {
        QuotaExtra {
            enabled: e.enabled,
            monthly_limit: e.monthly_limit,
            used_credits: e.used_credits,
            currency: e.currency,
            utilization: e.utilization,
        }
    }
}

impl Quota {
    fn new(q: aloud_quota::Quota, history: Vec<QuotaSample>) -> Self {
        Quota {
            five_hour: q.five_hour.map(Into::into),
            seven_day: q.seven_day.map(Into::into),
            seven_day_opus: q.seven_day_opus.map(Into::into),
            seven_day_sonnet: q.seven_day_sonnet.map(Into::into),
            seven_day_oauth_apps: q.seven_day_oauth_apps.map(Into::into),
            seven_day_cowork: q.seven_day_cowork.map(Into::into),
            seven_day_omelette: q.seven_day_omelette.map(Into::into),
            extra_usage_percent: q.extra_usage_percent,
            extra: q.extra.map(Into::into),
            history,
        }
    }
}

fn reset_secs(window: Option<&aloud_quota::Window>) -> Option<i64> {
    let at = window?.resets_at.as_deref()?;
    chrono::DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|t| t.timestamp())
}

/// Read the Claude Code token from the Keychain and fetch current usage.
///
/// The webview polls this. The first Keychain read may raise a macOS access
/// prompt — that's the OS's, and expected. An `Err` string is shown as-is in
/// the dashboard's empty state (e.g. "not signed in", "token expired").
#[tauri::command]
#[specta::specta]
pub async fn agent_quota() -> Result<Quota, String> {
    let quota = aloud_quota::fetch().await.map_err(|e| e.to_string())?;
    let sample = QuotaSample {
        at: chrono::Utc::now().timestamp(),
        five_hour: quota.five_hour.as_ref().map(|w| w.percent),
        five_hour_resets_at: reset_secs(quota.five_hour.as_ref()),
        seven_day: quota.seven_day.as_ref().map(|w| w.percent),
        seven_day_resets_at: reset_secs(quota.seven_day.as_ref()),
    };
    // A chart that cannot be saved is not a usage read that failed: show the
    // readings the file already had.
    let history = quota_history::record(sample).unwrap_or_else(|e| {
        tracing::warn!(%e, "quota history not saved");
        quota_history::load()
    });
    Ok(Quota::new(quota, history))
}
