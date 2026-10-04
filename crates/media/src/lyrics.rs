//! Synced lyrics from LRCLIB (free, no key): fetched in Rust because the
//! overlay's CSP is `default-src 'self'`, parsed from LRC, cached on disk per
//! track, and mapped from a playback position to the line being sung.
//!
//! This sends the track's title, artist, album and length to lrclib.net.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::now_playing::NowPlaying;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    pub ms: u32,
    /// Empty for an instrumental break; the view draws a rest.
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    /// Timed lines, empty when LRCLIB only has plain text.
    pub lines: Vec<LyricLine>,
    pub plain: Option<String>,
    pub instrumental: bool,
}

/// How long "LRCLIB has nothing for this track" is believed. Lyrics get added
/// over time, so a miss is not forever; a hit never expires.
const MISS_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Lyrics for `now`: the disk cache, else LRCLIB. `Ok(None)` is a definite
/// "no lyrics"; `Err` is a network failure and is not cached.
pub async fn lyrics(now: &NowPlaying) -> Result<Option<Lyrics>> {
    let path = cache_path(&now.key())?;
    if let Some(cached) = read_cache(&path) {
        return Ok(cached);
    }
    let found = fetch(now).await?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(&found) {
        let _ = std::fs::write(&path, json);
    }
    Ok(found)
}

fn cache_path(key: &str) -> Result<PathBuf> {
    Ok(aloud_notes::data_dir()?
        .join("lyrics")
        .join(format!("{:016x}.json", fnv1a(key))))
}

/// Stable across builds, unlike `DefaultHasher`, so a cache outlives an update.
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// `Some(entry)` when the cache holds a believable answer, which may itself
/// be "no lyrics".
fn read_cache(path: &std::path::Path) -> Option<Option<Lyrics>> {
    let entry: Option<Lyrics> = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    if entry.is_none() {
        let age = SystemTime::now()
            .duration_since(std::fs::metadata(path).ok()?.modified().ok()?)
            .unwrap_or_default();
        if age > MISS_TTL {
            return None;
        }
    }
    Some(entry)
}

/// The slice of an LRCLIB record this uses.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    duration: Option<f64>,
    #[serde(default)]
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

async fn fetch(now: &NowPlaying) -> Result<Option<Lyrics>> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("Aloud/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(8))
        .build()?;
    let seconds = (now.duration_ms / 1000).to_string();

    // The exact lookup first: it matches on duration, so a live cut does not
    // answer for the studio take. Album is sent as well, and dropped on a miss
    // because a "(Deluxe)" album name is the usual reason for one.
    for with_album in [true, false] {
        let mut query = vec![
            ("track_name", now.title.as_str()),
            ("artist_name", now.artist.as_str()),
            ("duration", seconds.as_str()),
        ];
        if with_album && !now.album.is_empty() {
            query.push(("album_name", now.album.as_str()));
        } else if with_album {
            continue;
        }
        let response = client
            .get("https://lrclib.net/api/get")
            .query(&query)
            .send()
            .await
            .context("asking LRCLIB")?;
        if response.status().is_success() {
            let record: Record = response.json().await.context("reading LRCLIB's answer")?;
            return Ok(Some(lyrics_of(&record)));
        }
        anyhow::ensure!(
            response.status() == reqwest::StatusCode::NOT_FOUND,
            "LRCLIB answered {}",
            response.status()
        );
    }

    let records: Vec<Record> = client
        .get("https://lrclib.net/api/search")
        .query(&[("track_name", &now.title), ("artist_name", &now.artist)])
        .send()
        .await
        .context("searching LRCLIB")?
        .error_for_status()?
        .json()
        .await
        .context("reading LRCLIB's search")?;
    Ok(best_match(&records, now.duration_ms / 1000).map(lyrics_of))
}

/// A record with timed lines whose length is within 3 s of ours, else any with
/// timed lines, else any with text — never a different song's lyrics for want
/// of a duration match when nothing closer exists.
fn best_match(records: &[Record], seconds: u32) -> Option<&Record> {
    let timed = |r: &&Record| {
        r.synced_lyrics
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
    };
    let close = |r: &&Record| {
        r.duration
            .is_some_and(|d| (d - f64::from(seconds)).abs() <= 3.0)
    };
    records
        .iter()
        .find(|r| timed(r) && close(r))
        .or_else(|| records.iter().find(timed))
        .or_else(|| {
            records
                .iter()
                .find(|r| r.plain_lyrics.is_some() || r.instrumental)
        })
}

fn lyrics_of(record: &Record) -> Lyrics {
    Lyrics {
        lines: record
            .synced_lyrics
            .as_deref()
            .map(parse_lrc)
            .unwrap_or_default(),
        plain: record.plain_lyrics.clone().filter(|s| !s.trim().is_empty()),
        instrumental: record.instrumental,
    }
}

/// LRC text as timed lines in time order. A line may carry several stamps
/// (`[00:12.00][01:30.50]chorus`) and becomes one entry per stamp; `[ar:…]`
/// style tags are skipped.
///
/// ponytail: no `[offset:]` tag support; LRCLIB's files do not use it.
pub fn parse_lrc(text: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(after) = rest.strip_prefix('[') {
            let Some((tag, tail)) = after.split_once(']') else {
                break;
            };
            // Not a stamp: a metadata tag like [ar:..], or the words themselves.
            let Some(ms) = parse_stamp(tag) else { break };
            stamps.push(ms);
            rest = tail;
        }
        let words = rest.trim();
        lines.extend(stamps.into_iter().map(|ms| LyricLine {
            ms,
            text: words.to_string(),
        }));
    }
    // Stable, so two lines sharing a stamp stay in file order.
    lines.sort_by_key(|l| l.ms);
    lines
}

/// `mm:ss`, `mm:ss.x`, `mm:ss.xx` (centiseconds) or `mm:ss.xxx` (milliseconds).
fn parse_stamp(tag: &str) -> Option<u32> {
    let (minutes, seconds) = tag.split_once(':')?;
    let (seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    if !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // Padded right to milliseconds: ".4" is 400, ".45" is 450, ".456" is 456.
    let fraction: u32 = format!("{:0<3}", fraction.get(..3).unwrap_or(fraction))
        .parse()
        .ok()?;
    let minutes: u32 = minutes.parse().ok()?;
    let seconds: u32 = seconds.parse().ok()?;
    Some((minutes * 60 + seconds) * 1000 + fraction)
}

/// The index of the line being sung at `position_ms`: the last one that has
/// started. None before the first line.
pub fn line_at(lines: &[LyricLine], position_ms: u32) -> Option<usize> {
    lines
        .partition_point(|l| l.ms <= position_ms)
        .checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(ms: u32, text: &str) -> LyricLine {
        LyricLine {
            ms,
            text: text.into(),
        }
    }

    /// Guards: ".4" read as 4 ms and ".45" as 45 ms put every line a fraction
    /// of a second early, which is just wrong enough to be believed.
    #[test]
    fn fractions_are_padded_to_milliseconds() {
        assert_eq!(parse_stamp("01:02.4"), Some(62_400));
        assert_eq!(parse_stamp("01:02.45"), Some(62_450));
        assert_eq!(parse_stamp("01:02.456"), Some(62_456));
        assert_eq!(parse_stamp("01:02"), Some(62_000));
        assert_eq!(parse_stamp("ar:Someone"), None);
        assert_eq!(parse_stamp("00:xx.00"), None);
    }

    #[test]
    fn repeated_stamps_become_one_line_each_in_time_order() {
        let lrc = "[ar:The Artist]\n[ti:Song]\n[00:30.00]second\n[00:10.00][01:00.50]chorus\n[00:20.00]\nno stamp";
        assert_eq!(
            parse_lrc(lrc),
            vec![
                line(10_000, "chorus"),
                line(20_000, ""),
                line(30_000, "second"),
                line(60_500, "chorus"),
            ]
        );
    }

    /// Guards: the boundary lines. A position exactly on a stamp is already
    /// that line, and before the first stamp nothing is highlighted.
    #[test]
    fn the_line_is_the_last_one_started() {
        let lines = [
            line(1000, "a"),
            line(2000, "b"),
            line(2000, "c"),
            line(5000, "d"),
        ];
        assert_eq!(line_at(&lines, 0), None);
        assert_eq!(line_at(&lines, 999), None);
        assert_eq!(line_at(&lines, 1000), Some(0));
        assert_eq!(line_at(&lines, 1999), Some(0));
        assert_eq!(line_at(&lines, 2000), Some(2));
        assert_eq!(line_at(&lines, 999_999), Some(3));
        assert_eq!(line_at(&[], 5), None);
    }

    fn record(duration: f64, synced: Option<&str>, plain: Option<&str>) -> Record {
        Record {
            duration: Some(duration),
            instrumental: false,
            plain_lyrics: plain.map(Into::into),
            synced_lyrics: synced.map(Into::into),
        }
    }

    #[test]
    fn search_prefers_timed_lyrics_of_the_right_length() {
        let records = [
            record(180.0, Some("[00:01.00]live"), None),
            record(201.0, Some("[00:01.00]studio"), None),
            record(201.0, None, Some("plain")),
        ];
        let best = best_match(&records, 200).unwrap();
        assert_eq!(best.synced_lyrics.as_deref(), Some("[00:01.00]studio"));

        // Nothing close: still timed lyrics over plain, never nothing.
        let best = best_match(&records, 400).unwrap();
        assert_eq!(best.synced_lyrics.as_deref(), Some("[00:01.00]live"));
        assert!(best_match(&[], 1).is_none());
    }

    #[test]
    fn the_cache_key_is_stable() {
        // A fixed value: if this moves, every cached lyric is orphaned.
        assert_eq!(
            fnv1a("Spotify|Song|Artist|200"),
            fnv1a("Spotify|Song|Artist|200")
        );
        assert_eq!(format!("{:016x}", fnv1a("")), "cbf29ce484222325");
    }

    #[test]
    fn a_fresh_miss_is_believed_and_a_stale_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.json");
        std::fs::write(&path, "null").unwrap();
        assert_eq!(read_cache(&path), Some(None));

        let old = SystemTime::now() - MISS_TTL - Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert_eq!(read_cache(&path), None);

        std::fs::write(&path, r#"{"lines":[],"plain":"hi","instrumental":false}"#).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert!(
            matches!(read_cache(&path), Some(Some(_))),
            "a hit does not expire"
        );
    }
}
