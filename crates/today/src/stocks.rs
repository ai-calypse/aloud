//! A small ticker over Yahoo Finance's public chart endpoint (no key).
//!
//! It is unofficial and can change or throttle without notice, so every
//! failure is a row that says why rather than a broken view, and nothing polls
//! in the background: quotes are fetched when someone is looking.
//!
//! A stale price must never look live, so a quote carries what the market is
//! doing (open, pre-market, after hours, closed) and whether the last trade is
//! old for a market that should be trading.

use anyhow::{anyhow, Context, Result};
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};

use crate::client;

/// A last trade older than this, while the market is open, is "delayed".
const STALE_SECS: i64 = 20 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MarketState {
    Open,
    Pre,
    After,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Ticker {
    pub symbol: String,
    pub name: String,
    pub price: Option<f64>,
    /// Against the previous close.
    pub change_pct: Option<f64>,
    pub state: Option<MarketState>,
    /// Open, but the last trade is old: do not read the price as now.
    pub delayed: bool,
    /// Why there is no price, in a sentence.
    pub error: Option<String>,
    pub currency: Option<String>,
    /// Yesterday's close: the baseline the day's line is drawn against.
    pub previous_close: Option<f64>,
    /// The regular session so far, empty when there are fewer than two bars.
    pub series: Vec<SeriesPoint>,
}

/// One bar of the day's line. `x` is where it falls in the regular session
/// (0 at the open, 1 at the close), worked out here so the webview needs no
/// clock and a half-finished day draws as a half-finished line.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SeriesPoint {
    pub x: f64,
    pub y: f64,
}

/// Upper-cases and checks a symbol before it goes into a URL or a file:
/// letters, digits and the `. - ^ =` that tickers, indices and crypto pairs
/// (`BRK.B`, `^GSPC`, `BTC-USD`) use.
pub fn normalize(input: &str) -> Option<String> {
    let s = input.trim().to_uppercase();
    let ok = (1..=12).contains(&s.len())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-^=".contains(c));
    ok.then_some(s)
}

#[derive(Deserialize)]
struct Chart {
    chart: Outer,
}
#[derive(Deserialize)]
struct Outer {
    result: Option<Vec<Result0>>,
    error: Option<Err0>,
}
#[derive(Deserialize)]
struct Err0 {
    description: Option<String>,
}
#[derive(Deserialize)]
struct Result0 {
    meta: Meta,
    #[serde(default)]
    timestamp: Vec<i64>,
    indicators: Option<Indicators>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    symbol: String,
    long_name: Option<String>,
    short_name: Option<String>,
    currency: Option<String>,
    regular_market_price: Option<f64>,
    chart_previous_close: Option<f64>,
    current_trading_period: Option<Periods>,
}
#[derive(Deserialize)]
struct Periods {
    pre: Span,
    regular: Span,
    post: Span,
}
#[derive(Deserialize)]
struct Span {
    start: i64,
    end: i64,
}
#[derive(Deserialize)]
struct Indicators {
    quote: Vec<Quote>,
}
#[derive(Deserialize)]
struct Quote {
    close: Vec<Option<f64>>,
}

fn state(now: i64, p: &Periods) -> MarketState {
    let within = |s: &Span| (s.start..s.end).contains(&now);
    if within(&p.regular) {
        MarketState::Open
    } else if within(&p.pre) {
        MarketState::Pre
    } else if within(&p.post) {
        MarketState::After
    } else {
        MarketState::Closed
    }
}

/// The regular session's bars as a line.
///
/// `currentTradingPeriod` names the *next* session once one has closed (the
/// recorded weekend fixture says Monday's while the bars are Friday's), so the
/// window is slid by whole days to the one the bars belong to. Pre- and
/// post-market bars fall outside it and are dropped: a thin extended-hours
/// tail would squash the real session's shape.
///
/// ponytail: the slide ignores a DST change between the two days, which shifts
/// the window an hour for one session; per-exchange tz data is the upgrade.
fn session_series(
    timestamps: &[i64],
    closes: &[Option<f64>],
    regular: Option<&Span>,
) -> Vec<SeriesPoint> {
    let bars: Vec<(i64, f64)> = timestamps
        .iter()
        .zip(closes)
        .filter_map(|(t, c)| c.map(|c| (*t, c)))
        .collect();
    let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
        return Vec::new();
    };
    let (start, end) = match regular {
        Some(s) => {
            let days = ((last.0 - s.start) as f64 / 86_400.0).round() as i64;
            (s.start + days * 86_400, s.end + days * 86_400)
        }
        None => (first.0, last.0),
    };
    let span = (end - start) as f64;
    let points: Vec<SeriesPoint> = bars
        .iter()
        .filter(|(t, _)| (start..=end).contains(t))
        .map(|(t, y)| SeriesPoint {
            x: (t - start) as f64 / span,
            y: *y,
        })
        .collect();
    if span > 0.0 && points.len() >= 2 {
        points
    } else {
        Vec::new()
    }
}

/// `now` is unix seconds, a parameter so the clock is testable.
fn parse(body: &str, now: i64) -> Result<Ticker> {
    let chart: Chart = serde_json::from_str(body).context("reading the quote")?;
    let r = chart
        .chart
        .result
        .and_then(|r| r.into_iter().next())
        .ok_or_else(|| {
            anyhow!(chart
                .chart
                .error
                .and_then(|e| e.description)
                .unwrap_or_else(|| "no such symbol".into()))
        })?;
    let market = r
        .meta
        .current_trading_period
        .as_ref()
        .map(|p| state(now, p));

    // The last bar, pre- and post-market included: what the market did most
    // recently, which is not what `regularMarketPrice` (the regular session's
    // last) says once it has closed for the day.
    let last_bar = r
        .indicators
        .as_ref()
        .and_then(|i| i.quote.first())
        .and_then(|q| {
            r.timestamp
                .iter()
                .zip(&q.close)
                .rev()
                .find_map(|(t, c)| c.map(|c| (*t, c)))
        });
    let price = match (market, last_bar) {
        (Some(MarketState::Pre | MarketState::After), Some((_, close))) => Some(close),
        _ => r.meta.regular_market_price.or(last_bar.map(|(_, c)| c)),
    };
    let change_pct = match (price, r.meta.chart_previous_close) {
        (Some(p), Some(prev)) if prev != 0.0 => Some((p - prev) / prev * 100.0),
        _ => None,
    };
    let delayed =
        market == Some(MarketState::Open) && last_bar.is_none_or(|(t, _)| now - t > STALE_SECS);

    let series = r
        .indicators
        .as_ref()
        .and_then(|i| i.quote.first())
        .map(|q| {
            session_series(
                &r.timestamp,
                &q.close,
                r.meta.current_trading_period.as_ref().map(|p| &p.regular),
            )
        })
        .unwrap_or_default();

    Ok(Ticker {
        name: r
            .meta
            .long_name
            .or(r.meta.short_name)
            .unwrap_or_else(|| r.meta.symbol.clone()),
        symbol: r.meta.symbol,
        price,
        change_pct,
        state: market,
        delayed,
        error: price.is_none().then(|| "no price yet".to_string()),
        currency: r.meta.currency,
        previous_close: r.meta.chart_previous_close,
        series,
    })
}

async fn one(symbol: &str, now: i64) -> Ticker {
    let fetched = async {
        let body = client()
            .get(format!(
                "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}"
            ))
            .query(&[
                ("interval", "5m"),
                ("range", "1d"),
                ("includePrePost", "true"),
            ])
            .send()
            .await?
            .text()
            .await?;
        parse(&body, now)
    }
    .await;
    fetched.unwrap_or_else(|e| Ticker {
        symbol: symbol.to_string(),
        name: symbol.to_string(),
        price: None,
        change_pct: None,
        state: None,
        delayed: false,
        error: Some(e.to_string()),
        currency: None,
        previous_close: None,
        series: Vec::new(),
    })
}

/// Quotes for `symbols`, in order, fetched together.
pub async fn quotes(symbols: &[String]) -> Vec<Ticker> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    join_all(symbols.iter().map(|s| one(s, now))).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // Trimmed from a live response. Regular session 13:30–20:00 UTC (9:30–16:00 ET).
    fn body(price: f64, last_bar_at: i64, last_close: f64) -> String {
        format!(
            r#"{{"chart":{{"result":[{{"meta":{{"symbol":"AAPL","longName":"Apple Inc.","currency":"USD",
            "regularMarketPrice":{price},"chartPreviousClose":100.0,
            "currentTradingPeriod":{{"pre":{{"start":1000,"end":2000}},
            "regular":{{"start":2000,"end":3000}},"post":{{"start":3000,"end":4000}}}}}},
            "timestamp":[2500,{last_bar_at}],"indicators":{{"quote":[{{"close":[101.0,{last_close}]}}]}}}}],
            "error":null}}}}"#
        )
    }

    #[test]
    fn the_market_state_follows_the_trading_periods_at_their_edges() {
        let p = Periods {
            pre: Span {
                start: 1000,
                end: 2000,
            },
            regular: Span {
                start: 2000,
                end: 3000,
            },
            post: Span {
                start: 3000,
                end: 4000,
            },
        };
        assert_eq!(state(999, &p), MarketState::Closed);
        assert_eq!(state(1000, &p), MarketState::Pre);
        assert_eq!(state(2000, &p), MarketState::Open);
        assert_eq!(state(2999, &p), MarketState::Open);
        assert_eq!(state(3000, &p), MarketState::After);
        assert_eq!(state(4000, &p), MarketState::Closed);
    }

    #[test]
    fn closed_shows_the_regular_close_and_after_hours_shows_the_last_trade() {
        let t = parse(&body(105.0, 3500, 106.5), 5000).unwrap();
        assert_eq!((t.state, t.price), (Some(MarketState::Closed), Some(105.0)));
        assert_eq!(t.change_pct, Some(5.0));

        let t = parse(&body(105.0, 3500, 106.5), 3600).unwrap();
        assert_eq!((t.state, t.price), (Some(MarketState::After), Some(106.5)));
    }

    /// The failure this guards: a price from an hour ago drawn like a live one
    /// because the market is open and the feed went quiet.
    #[test]
    fn an_open_market_with_an_old_last_trade_is_delayed() {
        let fresh = parse(&body(105.0, 2990, 105.0), 2999).unwrap();
        assert!(!fresh.delayed);
        let mut stale = body(105.0, 2000 - 1, 105.0);
        stale = stale.replace("3000}", "9000}"); // regular session stretches past `now`
        let t = parse(&stale, 2000 + STALE_SECS + 5).unwrap();
        assert_eq!(t.state, Some(MarketState::Open));
        assert!(t.delayed);
    }

    /// Recorded from Yahoo on a Monday morning for Friday's session: the
    /// trading period it names is Monday's, the bars are Friday's, and 79 of
    /// the 193 are regular-session. The failure this guards: a window taken
    /// verbatim from `currentTradingPeriod` puts no bar inside it and the
    /// chart comes back empty on every weekend and evening.
    #[test]
    fn a_closed_markets_chart_is_the_last_regular_session_not_the_next_one() {
        let body = include_str!("../tests/fixtures/aapl-1d-5m.json");
        let t = parse(body, 1_791_100_000).unwrap();
        assert_eq!(t.symbol, "AAPL");
        assert_eq!(t.series.len(), 79);
        let (first, last) = (t.series[0], t.series[78]);
        assert_eq!(first.x, 0.0); // the 9:30 bar
        assert!(
            last.x == 1.0,
            "the 16:00 bar is the right edge, got {}",
            last.x
        );
        assert!(t.series.windows(2).all(|w| w[0].x < w[1].x));
        assert_eq!(t.previous_close, Some(330.32));
        assert_eq!(t.state, Some(MarketState::Closed));
    }

    #[test]
    fn a_session_in_progress_stops_short_of_the_right_edge() {
        let r = |t: &[i64], c: &[Option<f64>]| {
            session_series(
                t,
                c,
                Some(&Span {
                    start: 2000,
                    end: 3000,
                }),
            )
        };
        let half = r(
            &[1500, 2000, 2250, 2500],
            &[Some(1.0), Some(2.0), None, Some(3.0)],
        );
        // The pre-market bar and the gap are gone; the line ends mid-session.
        assert_eq!(
            half,
            [
                SeriesPoint { x: 0.0, y: 2.0 },
                SeriesPoint { x: 0.5, y: 3.0 }
            ]
        );
        // One bar is a dot, not a line; no bars is nothing to draw.
        assert!(r(&[2000], &[Some(2.0)]).is_empty());
        assert!(r(&[], &[]).is_empty());
    }

    #[test]
    fn an_unknown_symbol_says_so_instead_of_failing() {
        let body = r#"{"chart":{"result":null,"error":{"code":"Not Found","description":"No data found, symbol may be delisted"}}}"#;
        let err = parse(body, 0).unwrap_err().to_string();
        assert!(err.contains("symbol may be delisted"), "got {err}");
        assert!(parse("<html>429</html>", 0).is_err());
    }

    #[test]
    fn symbols_are_checked_before_they_reach_a_url() {
        assert_eq!(normalize(" brk.b "), Some("BRK.B".into()));
        assert_eq!(normalize("^gspc"), Some("^GSPC".into()));
        assert_eq!(normalize("btc-usd"), Some("BTC-USD".into()));
        assert_eq!(normalize("AAPL/../x"), None);
        assert_eq!(normalize("a b"), None);
        assert_eq!(normalize(""), None);
        assert_eq!(normalize("THISISWAYTOOLONG"), None);
    }
}
