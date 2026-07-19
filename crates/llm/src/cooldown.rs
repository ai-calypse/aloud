//! Rate-limit circuit breaker, ported from `LLMCooldownManager.swift`.
//!
//! When a model returns 429, its reset time is read from the response headers
//! so the app knows exactly when the model is usable again and can fall back
//! to another model until then.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// No timing header present (rare): cool down briefly, then re-probe.
const DEFAULT_REPROBE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cooldown {
    pub duration: Duration,
    /// A spent daily (RPD) quota rather than a per-minute (TPM) one. Worth
    /// surfacing in settings: it will not clear on its own for hours.
    pub is_daily: bool,
}

/// Reads how long a model is rate-limited from a 429's headers.
///
/// Header precedence is deliberate and ported as-is: a spent daily quota is
/// classified first, *even when* `retry-after` is also present. Checking
/// `retry-after` first would hide a short near-reset daily window and stop it
/// from being persisted.
pub fn from_headers(headers: &reqwest::header::HeaderMap) -> Cooldown {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());

    // Daily (RPD) quota spent.
    let remaining =
        header("x-ratelimit-remaining-requests").and_then(|v| v.trim().parse::<f64>().ok());
    if remaining.is_some_and(|r| r <= 0.0) {
        if let Some(reset) = header("x-ratelimit-reset-requests").and_then(parse_duration) {
            return Cooldown {
                duration: reset,
                is_daily: true,
            };
        }
    }

    // retry-after is the authoritative wait Groq sets on a 429.
    if let Some(value) = header("retry-after").and_then(parse_duration) {
        return Cooldown {
            duration: value,
            is_daily: false,
        };
    }

    // Per-minute (TPM) reset, e.g. "7.66s".
    if let Some(value) = header("x-ratelimit-reset-tokens").and_then(parse_duration) {
        return Cooldown {
            duration: value,
            is_daily: false,
        };
    }

    Cooldown {
        duration: DEFAULT_REPROBE,
        is_daily: false,
    }
}

/// Parses a Groq duration: bare seconds ("2", "7.66"), a single suffixed unit
/// ("7.66s", "120ms"), or compound forms ("2m59.56s", "1h0m0s").
///
/// Returns None for empty, unrecognized, negative or non-finite input, so a
/// malformed header can never produce a negative or NaN cooldown.
fn parse_duration(value: &str) -> Option<Duration> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    // A bare number is plain seconds — the `retry-after` integer form.
    if let Ok(seconds) = trimmed.parse::<f64>() {
        return finite_non_negative(seconds);
    }

    let mut total = 0.0;
    let mut number = String::new();
    let mut matched_unit = false;
    let mut rest = trimmed;

    while let Some(c) = rest.chars().next() {
        if c.is_ascii_digit() || c == '.' {
            number.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        }

        let parsed: f64 = number.parse().ok()?;
        number.clear();

        // "ms" must be checked before the single-letter "m".
        let (multiplier, width) = if rest.starts_with("ms") {
            (0.001, 2)
        } else {
            match c {
                'h' => (3600.0, 1),
                'm' => (60.0, 1),
                's' => (1.0, 1),
                _ => return None, // unrecognized unit
            }
        };
        total += parsed * multiplier;
        matched_unit = true;
        rest = &rest[width..];
    }

    // Trailing digits with no unit ("2m30") are malformed.
    if !matched_unit || !number.is_empty() {
        return None;
    }
    finite_non_negative(total)
}

fn finite_non_negative(seconds: f64) -> Option<Duration> {
    (seconds.is_finite() && seconds >= 0.0).then(|| Duration::from_secs_f64(seconds))
}

/// Tracks which models are rate-limited and until when.
#[derive(Default)]
pub struct CooldownTracker {
    until: Mutex<HashMap<String, Instant>>,
}

impl CooldownTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, model: &str, cooldown: Cooldown) {
        let expires = Instant::now() + cooldown.duration;
        tracing::info!(
            model,
            seconds = cooldown.duration.as_secs(),
            daily = cooldown.is_daily,
            "model rate-limited"
        );
        if let Ok(mut map) = self.until.lock() {
            map.insert(model.to_string(), expires);
        }
    }

    /// Whether `model` is still cooling down. Expired entries are dropped.
    pub fn is_limited(&self, model: &str) -> bool {
        self.remaining(model).is_some()
    }

    /// How long `model` stays limited, or `None` if it is usable. Expired
    /// entries are dropped. The remaining time is what makes a rate-limit
    /// message actionable — "try again in 40s" rather than "try again".
    pub fn remaining(&self, model: &str) -> Option<Duration> {
        let Ok(mut map) = self.until.lock() else {
            return None; // a poisoned lock must not block all requests
        };
        let now = Instant::now();
        match map.get(model) {
            Some(expires) if *expires > now => Some(*expires - now),
            Some(_) => {
                map.remove(model);
                None
            }
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.insert(*k, HeaderValue::from_str(v).unwrap());
        }
        map
    }

    #[test]
    fn parses_bare_seconds() {
        assert_eq!(parse_duration("2"), Some(Duration::from_secs(2)));
        assert_eq!(parse_duration("7.66"), Some(Duration::from_secs_f64(7.66)));
    }

    #[test]
    fn parses_suffixed_units() {
        assert_eq!(parse_duration("7.66s"), Some(Duration::from_secs_f64(7.66)));
        assert_eq!(parse_duration("120ms"), Some(Duration::from_millis(120)));
    }

    #[test]
    fn parses_compound_durations() {
        assert_eq!(
            parse_duration("2m59.56s"),
            Some(Duration::from_secs_f64(179.56))
        );
        assert_eq!(parse_duration("1h0m0s"), Some(Duration::from_secs(3600)));
        assert_eq!(
            parse_duration("1h2m3.5s"),
            Some(Duration::from_secs_f64(3723.5))
        );
    }

    /// A malformed header must never yield a negative, NaN or infinite wait.
    #[test]
    fn rejects_malformed_input() {
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("  "), None);
        assert_eq!(parse_duration("-3"), None);
        assert_eq!(parse_duration("nan"), None);
        assert_eq!(parse_duration("inf"), None);
        assert_eq!(parse_duration("2x"), None);
        assert_eq!(parse_duration("2m30"), None); // trailing digits, no unit
    }

    /// The precedence that matters: a spent daily quota wins over retry-after,
    /// so a short near-reset daily window is still classified as daily.
    #[test]
    fn spent_daily_quota_outranks_retry_after() {
        let cooldown = from_headers(&headers(&[
            ("x-ratelimit-remaining-requests", "0"),
            ("x-ratelimit-reset-requests", "30s"),
            ("retry-after", "2"),
        ]));
        assert_eq!(cooldown.duration, Duration::from_secs(30));
        assert!(cooldown.is_daily);
    }

    #[test]
    fn retry_after_wins_when_daily_quota_remains() {
        let cooldown = from_headers(&headers(&[
            ("x-ratelimit-remaining-requests", "500"),
            ("retry-after", "2"),
            ("x-ratelimit-reset-tokens", "7.66s"),
        ]));
        assert_eq!(cooldown.duration, Duration::from_secs(2));
        assert!(!cooldown.is_daily);
    }

    #[test]
    fn falls_back_to_token_reset_then_default() {
        let tokens = from_headers(&headers(&[("x-ratelimit-reset-tokens", "7.66s")]));
        assert_eq!(tokens.duration, Duration::from_secs_f64(7.66));

        assert_eq!(from_headers(&HeaderMap::new()).duration, DEFAULT_REPROBE);
    }

    #[test]
    fn tracker_reports_and_expires_cooldowns() {
        let tracker = CooldownTracker::new();
        assert!(!tracker.is_limited("model-a"));

        tracker.set(
            "model-a",
            Cooldown {
                duration: Duration::from_secs(60),
                is_daily: false,
            },
        );
        assert!(tracker.is_limited("model-a"));
        assert!(!tracker.is_limited("model-b"));

        // An elapsed cooldown must clear.
        tracker.set(
            "model-c",
            Cooldown {
                duration: Duration::ZERO,
                is_daily: false,
            },
        );
        assert!(!tracker.is_limited("model-c"));
    }
}
