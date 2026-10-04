//! Video and audio playing in a browser tab (YouTube, Twitch, a podcast
//! page), read and driven through each browser's own scripting: a small
//! script runs *inside* each tab and reports its `<video>`/`<audio>`.
//!
//! No private API. It costs the user one switch per browser, which Aloud can
//! only point at and never flip: Brave/Chrome/Edge › View › Developer ›
//! "Allow JavaScript from Apple Events", Safari › Develop › the same words.
//! Until it is on, every browser reports [`IssueKind::JsOff`] and the view says
//! so, rather than saying "nothing playing" over a playing video.
//!
//! Privacy: only a tab that holds playing (or part-played) media is reported,
//! and only its title, host, position and the artwork address leave the
//! script. Page text, other tabs' URLs, cookies and forms are never read, and
//! the tab list's URLs are only filtered to http(s) inside osascript.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use aloud_native::{run_jxa, ScriptError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::now_playing::{NowPlaying, PlayState, Player, Song, Source};

/// The browsers with a scripting dictionary that can run JavaScript in a tab.
/// ponytail: four. Arc, Vivaldi and Opera share Chrome's dictionary, so adding
/// one is a row here and in `PROCESSES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Browser {
    Brave,
    Chrome,
    Edge,
    Safari,
}

impl Browser {
    pub const ALL: [Browser; 4] = [Browser::Brave, Browser::Chrome, Browser::Edge, Browser::Safari];

    /// The app's name: its process name and its scripting target.
    pub fn app(self) -> &'static str {
        match self {
            Browser::Brave => "Brave Browser",
            Browser::Chrome => "Google Chrome",
            Browser::Edge => "Microsoft Edge",
            Browser::Safari => "Safari",
        }
    }

    pub fn from_app(app: &str) -> Option<Browser> {
        Self::ALL.into_iter().find(|b| b.app() == app)
    }

    fn safari(self) -> bool {
        self == Browser::Safari
    }
}

/// Why a running browser could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    /// "Allow JavaScript from Apple Events" is off.
    JsOff,
    /// macOS Automation permission for the browser was refused.
    NotPermitted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub app: String,
    pub kind: IssueKind,
}

/// What the page script reports about the media element it chose.
#[derive(Debug, Deserialize)]
struct Page {
    host: String,
    title: String,
    cur: f64,
    dur: f64,
    paused: bool,
    rate: f64,
    /// YouTube video id, from the address bar.
    #[serde(default)]
    yt: String,
    #[serde(default)]
    channel: String,
    /// YouTube Music's player bar: title, and "Artist • Album • Year".
    #[serde(default)]
    mt: String,
    #[serde(default)]
    mb: String,
    #[serde(default)]
    art: String,
    #[serde(default)]
    site: String,
}

/// Picks the media element a tab means: a playing one over a paused one, then
/// the longest. Shared by the scan and the controls, so a button always lands
/// on the element the view is showing.
const PICK: &str = r#"
const pick = () => {
  let best = null, score = -1;
  for (const e of document.querySelectorAll("video,audio")) {
    const playing = !e.paused && !e.ended && e.readyState > 2;
    const started = e.paused && !e.ended && e.currentTime > 1;
    if (!playing && !started) continue;
    // A muted loop or autoplay is a page decoration (a hero clip, a hover
    // preview), not something the user is watching.
    if (e.muted && (e.loop || e.autoplay)) continue;
    if (e.closest("ytd-video-preview, #inline-preview-player")) continue;
    if (Number.isFinite(e.duration) && e.duration < 3) continue;
    const s = (playing ? 1e9 : 0) + (Number.isFinite(e.duration) ? e.duration : 1e6);
    if (s > score) { best = e; score = s; }
  }
  return best;
};"#;

const READ_PAGE: &str = r#"() => {
  const e = pick();
  if (!e) return null;
  const q = (s) => document.querySelector(s);
  const text = (s) => { const n = q(s); return n ? n.textContent.trim() : ""; };
  const host = location.host;
  const o = {
    host, title: document.title, cur: e.currentTime,
    dur: Number.isFinite(e.duration) ? e.duration : 0, paused: e.paused, rate: e.playbackRate,
  };
  if (/(^|\.)youtube\.com$/.test(host)) {
    const u = new URL(location.href);
    o.yt = u.searchParams.get("v") || ((u.pathname.match(/^\/(?:shorts|live|embed)\/([\w-]{11})/) || [])[1]) || "";
    if (host === "music.youtube.com") {
      o.mt = text("ytmusic-player-bar .title");
      o.mb = text("ytmusic-player-bar .byline");
    } else {
      o.channel = text("ytd-watch-metadata ytd-channel-name a") || text("ytd-channel-name a")
        || ((q('[itemprop="author"] link[itemprop="name"]') || {}).content || "");
    }
  } else {
    o.art = (q('meta[property="og:image"]') || {}).content || "";
    o.site = (q('meta[property="og:site_name"]') || {}).content || "";
  }
  return JSON.stringify(o);
}"#;

const CONTROL_PAGE: &str = r#"(a) => {
  if (a.host && location.host !== a.host) return "moved";
  const e = pick();
  if (!e) return "gone";
  const play = () => { const p = e.play(); if (p) p.catch(() => {}); };
  switch (a.cmd) {
    case "play": play(); break;
    case "pause": e.pause(); break;
    case "toggle": e.paused ? play() : e.pause(); break;
    case "seek": e.currentTime = a.value; break;
    case "speed": e.playbackRate = a.value; break;
    case "previous": e.currentTime = 0; break;
    case "next": {
      const b = document.querySelector(".ytp-next-button, ytmusic-player-bar .next-button");
      if (!b) return "unsupported";
      b.click();
      break;
    }
  }
  return "ok";
}"#;

/// The page function as source: `PICK` first, so the body can call `pick()`.
fn page_fn(body: &str) -> String {
    format!("(function(){{ {PICK} return ({body}); }})()")
}

/// Runs `ARG.js` (a page function) in each tab and returns the ones that
/// answered with a media report. With `ARG.only` it asks just those tabs, the
/// cheap refresh between full walks. `errors` is how the caller learns that
/// JavaScript from Apple Events is off; one failing tab ends its window,
/// since the setting is per browser and the rest would fail the same way.
///
/// Window and Chromium tab ids are stable; Safari has no tab ids, so its tabs
/// are told apart by position, and the page script's host check catches a tab
/// that has moved.
const SCAN: &str = r#"
const A = Application(ARG.app);
const tabs = [], errors = [];
let ok = 0;
const ask = (tab) => ARG.safari
  ? A.doJavaScript(ARG.js, { in: tab })
  : tab.execute({ javascript: ARG.js });
const probe = (w, t, tab) => {
  try {
    const r = ask(tab);
    ok++;
    if (typeof r === "string" && r[0] === "{") tabs.push({ w: String(w), t: String(t), page: JSON.parse(r) });
    return true;
  } catch (e) {
    const m = String(e.message);
    if (errors.length < 3 && !errors.includes(m)) errors.push(m);
    return false;
  }
};
if (ARG.only) {
  for (const o of ARG.only) {
    try {
      const w = A.windows.byId(Number(o.w));
      probe(o.w, o.t, ARG.safari ? w.tabs[Number(o.t) - 1] : w.tabs.byId(Number(o.t)));
    } catch (e) { /* the tab or window is gone */ }
  }
} else {
  for (const w of A.windows()) {
    let wid, list, urls, ids;
    try { wid = w.id(); list = w.tabs; urls = list.url(); ids = ARG.safari ? null : list.id(); } catch (e) { continue; }
    for (let i = 0; i < urls.length; i++) {
      if (!/^(https?|file):/i.test(urls[i] || "")) continue;
      if (!probe(wid, ARG.safari ? i + 1 : ids[i], list[i])) break;
    }
  }
}
return JSON.stringify({ tabs, errors, ok });"#;

const CONTROL: &str = r#"
const A = Application(ARG.app);
const w = A.windows.byId(Number(ARG.w));
const tab = ARG.safari ? w.tabs[Number(ARG.t) - 1] : w.tabs.byId(Number(ARG.t));
const code = "(" + ARG.js + ")(" + JSON.stringify(ARG.args) + ")";
return String(ARG.safari ? A.doJavaScript(code, { in: tab }) : tab.execute({ javascript: code }));"#;

#[derive(Debug, Deserialize)]
struct Reply {
    #[serde(default)]
    tabs: Vec<Tab>,
    #[serde(default)]
    errors: Vec<String>,
    #[serde(default)]
    ok: u32,
}

#[derive(Debug, Deserialize)]
struct Tab {
    w: String,
    t: String,
    page: Page,
}

/// True for the error a browser gives while "Allow JavaScript from Apple
/// Events" is off. Chromium says "...turned off. ...go to View > Developer >
/// Allow JavaScript from Apple Events"; Safari "You must enable 'Allow
/// JavaScript from Apple Events'". Both carry the setting's name.
pub fn is_js_off(message: &str) -> bool {
    message.contains("Allow JavaScript from Apple Events")
}

/// Reads one scan's JSON into hits, or the issue that stopped it. An answer
/// with no successful tab and a "JavaScript is off" error is `JsOff`; an
/// answer with simply no media is an empty scan.
fn parse_scan(app: &str, out: &str) -> Result<Vec<NowPlaying>, IssueKind> {
    let Ok(reply) = serde_json::from_str::<Reply>(out) else {
        return Ok(Vec::new());
    };
    if reply.ok == 0 && reply.errors.iter().any(|e| is_js_off(e)) {
        return Err(IssueKind::JsOff);
    }
    Ok(reply
        .tabs
        .into_iter()
        .filter_map(|t| {
            let source = Source {
                app: app.into(),
                host: Some(t.page.host.clone()),
                tab: Some(format!("{}/{}", t.w, t.t)),
            };
            now_playing(source, t.page)
        })
        .collect())
}

fn now_playing(source: Source, p: Page) -> Option<NowPlaying> {
    let youtube = is_youtube(&p.host);
    let raw = if p.mt.is_empty() { &p.title } else { &p.mt };
    let mut title = clean_title(&p.host, raw);
    if title.is_empty() {
        title.clone_from(&p.host);
    }
    let artist = if p.host == "music.youtube.com" {
        p.mb.split('•').next().unwrap_or_default().trim().to_string()
    } else if youtube {
        p.channel.trim().trim_end_matches(" - Topic").to_string()
    } else {
        p.site.trim().to_string()
    };
    let duration_ms = (p.dur.max(0.0) * 1000.0) as u32;
    let artwork_url = if youtube && is_video_id(&p.yt) {
        Some(format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", p.yt))
    } else {
        Some(p.art.clone()).filter(|a| !youtube && a.starts_with("https://"))
    };
    let song = youtube
        .then(|| parse_song(&p, &title, &artist, duration_ms))
        .flatten();
    Some(NowPlaying {
        player: Player::Browser,
        state: if p.paused { PlayState::Paused } else { PlayState::Playing },
        title,
        artist,
        album: String::new(),
        duration_ms,
        position_ms: (p.cur.max(0.0) * 1000.0) as u32,
        speed_pct: (p.rate * 100.0).round().clamp(1.0, 1000.0) as u16,
        artwork_url,
        source,
        song,
    })
}

fn is_youtube(host: &str) -> bool {
    let host = host.split(':').next().unwrap_or(host);
    host == "youtube.com" || host.ends_with(".youtube.com")
}

/// The id lands in a URL we fetch, so it is checked rather than trusted.
fn is_video_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A tab's title as the thing playing: without YouTube's " - YouTube" and its
/// unread-notification counter "(3) ".
///
/// ponytail: the counter is stripped from any YouTube title that opens with
/// "(digits)", so a video that really starts "(500) Days of Summer" loses it.
pub fn clean_title(host: &str, raw: &str) -> String {
    let mut t = raw.trim();
    if is_youtube(host) {
        if let Some((n, tail)) = t.strip_prefix('(').and_then(|r| r.split_once(')')) {
            let digits = n.trim_end_matches('+');
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                t = tail.trim_start();
            }
        }
        for suffix in [" - YouTube Music", " - YouTube"] {
            if let Some(rest) = t.strip_suffix(suffix) {
                t = rest.trim_end();
                break;
            }
        }
    }
    t.to_string()
}

/// A track and artist for LRCLIB, only when the page names them cleanly:
/// YouTube Music's player bar, an auto-generated "Artist - Topic" channel, or
/// an "Artist - Title" video from a channel that is that artist's. Anything
/// else is None, because wrong lyrics are worse than none.
fn parse_song(p: &Page, title: &str, artist: &str, duration_ms: u32) -> Option<Song> {
    // A song, not a podcast or a livestream: "Name #12 - Guest" is two parts too.
    if !(30_000..=900_000).contains(&duration_ms) {
        return None;
    }
    if p.host == "music.youtube.com" {
        return (!title.is_empty() && !artist.is_empty()).then(|| Song {
            title: title.into(),
            artist: artist.into(),
        });
    }
    if p.channel.trim().ends_with(" - Topic") {
        return (!title.is_empty() && !artist.is_empty()).then(|| Song {
            title: title.into(),
            artist: artist.into(),
        });
    }
    let stripped = strip_noise(title);
    let (a, t) = split_once_only(&stripped)?;
    related(artist, a).then(|| Song {
        title: t.into(),
        artist: a.into(),
    })
}

/// Splits "A - B" into A and B, or None when there is not exactly one dash
/// (an "A - B - C" title has no clean answer) or a side is empty.
fn split_once_only(s: &str) -> Option<(&str, &str)> {
    let mut found = s
        .match_indices(" - ")
        .chain(s.match_indices(" \u{2013} "));
    let (i, sep) = found.next()?;
    if found.next().is_some() {
        return None;
    }
    let (a, t) = (s[..i].trim(), s[i + sep.len()..].trim());
    (!a.is_empty() && !t.is_empty()).then_some((a, t))
}

/// Removes "(Official Video)", "[Lyrics]", "(Audio)" and the like. A bracket
/// that carries anything else ("(feat. X)", "(Remix)") is part of the title.
fn strip_noise(title: &str) -> String {
    const NOISE: [&str; 8] = [
        "official", "lyric", "video", "audio", "visualizer", "4k", "hd", "hq",
    ];
    let mut out = String::new();
    let mut rest = title;
    while let Some(open) = rest.find(['(', '[']) {
        let close_ch = if rest[open..].starts_with('(') { ')' } else { ']' };
        let Some(len) = rest[open..].find(close_ch) else {
            break;
        };
        let inner = rest[open + 1..open + len].to_lowercase();
        out.push_str(&rest[..open]);
        if !NOISE.iter().any(|n| inner.contains(n)) {
            out.push_str(&rest[open..=open + len]);
        }
        rest = &rest[open + len + 1..];
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Is `channel` plausibly the artist's own: one contains the other once
/// "VEVO"/"Official" decoration and punctuation are gone ("AdeleVEVO" and
/// "Adele"). A label's or a lyric channel's name does not, and its video is
/// left without lyrics.
fn related(channel: &str, artist: &str) -> bool {
    let norm = |s: &str| {
        let s = s.to_lowercase();
        let s = s.trim_end_matches("vevo").trim_end_matches("official");
        s.chars().filter(|c| c.is_alphanumeric()).collect::<String>()
    };
    let (c, a) = (norm(channel), norm(artist));
    c.len() >= 3 && a.len() >= 3 && (c.contains(&a) || a.contains(&c))
}

// ---- scanning -------------------------------------------------------------

/// A browser's last verdict, so a browser that cannot be read costs one call
/// every few minutes, not one per poll.
static ISSUES: Mutex<Option<HashMap<Browser, (IssueKind, Instant)>>> = Mutex::new(None);
/// When each browser was last walked tab by tab, and where its media was.
static WALKED: Mutex<Option<HashMap<Browser, (Instant, Vec<String>)>>> = Mutex::new(None);

/// How long "JavaScript is off" is believed before the browser is asked again.
const RECHECK: Duration = Duration::from_secs(3 * 60);
/// Between full walks only the tabs that had media are asked again. A new
/// video is therefore found within this long, and a playing one is followed
/// every poll for the cost of one tab, however many are open.
const WALK_EVERY: Duration = Duration::from_secs(3);

fn lock<T>(m: &'static Mutex<Option<HashMap<Browser, T>>>) -> std::sync::MutexGuard<'static, Option<HashMap<Browser, T>>> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Forgets every verdict, for "I turned it on" in the view.
pub fn recheck() {
    *lock(&ISSUES) = None;
    *lock(&WALKED) = None;
}

/// Processes to look for. One `pgrep` for all of them: scripting a browser
/// that is not running would launch it.
#[cfg(target_os = "macos")]
fn running() -> Vec<Browser> {
    let pattern = Browser::ALL.map(Browser::app).join("|");
    std::process::Command::new("pgrep")
        .args(["-lx", &pattern])
        .output()
        .map(|o| parse_running(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

#[cfg(not(target_os = "macos"))]
fn running() -> Vec<Browser> {
    Vec::new()
}

/// `pgrep -l` lines, "pid name".
fn parse_running(out: &str) -> Vec<Browser> {
    let names: Vec<&str> = out
        .lines()
        .filter_map(|l| l.trim().split_once(' ').map(|(_, n)| n.trim()))
        .collect();
    Browser::ALL
        .into_iter()
        .filter(|b| names.contains(&b.app()))
        .collect()
}

/// Every running browser's media tabs, and the browsers that could not be
/// read. Returns at once when no browser is running.
pub fn scan() -> (Vec<NowPlaying>, Vec<Issue>) {
    let (mut hits, mut issues) = (Vec::new(), Vec::new());
    let running = running();
    // A browser that quit and came back has a fresh setting and permission.
    lock(&ISSUES)
        .get_or_insert_default()
        .retain(|b, _| running.contains(b));
    lock(&WALKED)
        .get_or_insert_default()
        .retain(|b, _| running.contains(b));
    for browser in running {
        let known = lock(&ISSUES).get_or_insert_default().get(&browser).copied();
        if let Some((kind, _)) = known.filter(|(_, at)| at.elapsed() < RECHECK) {
            issues.push(Issue { app: browser.app().into(), kind });
            continue;
        }
        match scan_browser(browser) {
            Ok(found) => {
                lock(&ISSUES).get_or_insert_default().remove(&browser);
                hits.extend(found);
            }
            Err(kind) => {
                lock(&ISSUES)
                    .get_or_insert_default()
                    .insert(browser, (kind, Instant::now()));
                issues.push(Issue { app: browser.app().into(), kind });
            }
        }
    }
    (hits, issues)
}

fn scan_browser(browser: Browser) -> Result<Vec<NowPlaying>, IssueKind> {
    let only = lock(&WALKED)
        .get_or_insert_default()
        .get(&browser)
        .filter(|(at, tabs)| at.elapsed() < WALK_EVERY && !tabs.is_empty())
        .map(|(_, tabs)| tabs.clone());
    let hits = scan_app(browser.app(), browser.safari(), only.as_deref())?;
    if only.is_none() {
        let tabs = hits.iter().filter_map(|n| n.source.tab.clone()).collect();
        lock(&WALKED)
            .get_or_insert_default()
            .insert(browser, (Instant::now(), tabs));
    }
    Ok(hits)
}

/// One osascript call over `app`'s tabs (`only`: "window/tab" ids, else all).
/// Public for the example, which scans a browser by any name.
pub fn scan_app(app: &str, safari: bool, only: Option<&[String]>) -> Result<Vec<NowPlaying>, IssueKind> {
    let only: Option<Vec<Value>> = only.map(|ids| {
        ids.iter()
            .filter_map(|id| id.split_once('/'))
            .map(|(w, t)| json!({ "w": w, "t": t }))
            .collect()
    });
    let arg = json!({ "app": app, "safari": safari, "js": page_fn(READ_PAGE), "only": only });
    match run_jxa(app, SCAN, &arg) {
        Ok(out) => {
            parse_scan(app, &out)
        }
        Err(ScriptError::NotPermitted { .. }) => Err(IssueKind::NotPermitted),
        Err(e) => {
            tracing::debug!(%e, app, "browser media scan failed");
            Ok(Vec::new())
        }
    }
}

// ---- controls -------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmd {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    SeekSeconds(f64),
    Speed(f64),
}

impl Cmd {
    fn args(self, host: &str) -> Value {
        let (cmd, value) = match self {
            Cmd::Play => ("play", 0.0),
            Cmd::Pause => ("pause", 0.0),
            Cmd::Toggle => ("toggle", 0.0),
            Cmd::Next => ("next", 0.0),
            Cmd::Previous => ("previous", 0.0),
            Cmd::SeekSeconds(s) => ("seek", s),
            Cmd::Speed(r) => ("speed", r),
        };
        json!({ "cmd": cmd, "value": value, "host": host })
    }
}

/// Sends `cmd` to the exact window and tab `source` names.
pub fn control(source: &Source, cmd: Cmd) -> Result<(), ScriptError> {
    let failed = |m: &str| ScriptError::Failed(m.into());
    let browser = Browser::from_app(&source.app).ok_or_else(|| failed("not a browser"))?;
    let (w, t) = source
        .tab
        .as_deref()
        .and_then(|id| id.split_once('/'))
        .ok_or_else(|| failed("no tab to control"))?;
    let arg = json!({
        "app": browser.app(), "safari": browser.safari(), "w": w, "t": t,
        "js": page_fn(CONTROL_PAGE),
        "args": cmd.args(source.host.as_deref().unwrap_or_default()),
    });
    match run_jxa(browser.app(), CONTROL, &arg)?.as_str() {
        "ok" => Ok(()),
        "unsupported" => Err(failed("this page has no next button")),
        _ => Err(failed("that tab changed; the next reading will find the video")),
    }
}

/// Brings `app` forward so the user can reach its menu bar. Only a browser
/// Aloud scans, so this cannot be turned into "open anything".
pub fn open(app: &str) {
    #[cfg(target_os = "macos")]
    if Browser::from_app(app).is_some() {
        let _ = std::process::Command::new("open").args(["-a", app]).status();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(host: &str, title: &str) -> Page {
        Page {
            host: host.into(),
            title: title.into(),
            cur: 12.5,
            dur: 213.0,
            paused: false,
            rate: 1.25,
            yt: "dQw4w9WgXcQ".into(),
            channel: String::new(),
            mt: String::new(),
            mb: String::new(),
            art: String::new(),
            site: String::new(),
        }
    }

    fn source() -> Source {
        Source { app: "Brave Browser".into(), host: None, tab: Some("1/2".into()) }
    }

    /// Guards: the notification counter and site suffix showing up as part of
    /// the song ("(3) Song - YouTube" under the artwork).
    #[test]
    fn youtube_titles_lose_the_counter_and_the_suffix() {
        let yt = "www.youtube.com";
        assert_eq!(clean_title(yt, "(3) Never Gonna Give You Up - YouTube"), "Never Gonna Give You Up");
        assert_eq!(clean_title(yt, "(99+) A (Live) - YouTube"), "A (Live)");
        assert_eq!(clean_title("music.youtube.com", "Song - YouTube Music"), "Song");
        assert_eq!(clean_title(yt, "Plain"), "Plain");
        // Other sites keep their own punctuation.
        assert_eq!(clean_title("example.com", "(3) Inbox - YouTube"), "(3) Inbox - YouTube");
    }

    #[test]
    fn a_youtube_page_becomes_a_track_with_a_thumbnail_and_speed() {
        let mut p = page("www.youtube.com", "(2) Rick Astley - Never Gonna Give You Up (Official Video) - YouTube");
        p.channel = "Rick Astley".into();
        let n = now_playing(source(), p).unwrap();
        assert_eq!(n.title, "Rick Astley - Never Gonna Give You Up (Official Video)");
        assert_eq!(n.artist, "Rick Astley");
        assert_eq!(n.artwork_url.as_deref(), Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg"));
        assert_eq!((n.position_ms, n.duration_ms, n.speed_pct), (12_500, 213_000, 125));
        assert_eq!(n.state, PlayState::Playing);
        let song = n.song.unwrap();
        assert_eq!((song.artist.as_str(), song.title.as_str()), ("Rick Astley", "Never Gonna Give You Up"));
    }

    /// Guards: lyrics for the wrong song, which is worse than none.
    #[test]
    fn lyrics_are_only_looked_up_for_a_clean_parse() {
        let song = |title: &str, channel: &str, dur: f64| {
            let mut p = page("www.youtube.com", &format!("{title} - YouTube"));
            p.channel = channel.into();
            p.dur = dur;
            now_playing(source(), p).unwrap().song
        };
        // The artist's own channel, and a VEVO channel.
        assert!(song("Adele - Hello", "Adele", 300.0).is_some());
        assert!(song("Adele - Hello (Official Music Video)", "AdeleVEVO", 300.0).is_some());
        // A label or lyric channel is not the artist's.
        assert!(song("Adele - Hello", "Lyric Library", 300.0).is_none());
        // Three parts, no dash, a podcast's length, a livestream.
        assert!(song("A - B - C", "A", 300.0).is_none());
        assert!(song("Hello", "Adele", 300.0).is_none());
        assert!(song("Adele - Hello", "Adele", 7200.0).is_none());
        assert!(song("Adele - Hello", "Adele", 0.0).is_none());
        // A bracket that is part of the title survives.
        let s = song("Adele - Hello (Live at Home)", "Adele", 300.0).unwrap();
        assert_eq!(s.title, "Hello (Live at Home)");
        // Topic channels are auto-generated: the title is the track.
        let mut p = page("www.youtube.com", "Hello - YouTube");
        p.channel = "Adele - Topic".into();
        let n = now_playing(source(), p).unwrap();
        assert_eq!(n.artist, "Adele");
        assert_eq!(n.song.unwrap().artist, "Adele");
    }

    #[test]
    fn youtube_music_reads_the_player_bar() {
        let mut p = page("music.youtube.com", "Fallback - YouTube Music");
        p.mt = "Hello".into();
        p.mb = "Adele • 25 • 2015".into();
        let n = now_playing(source(), p).unwrap();
        assert_eq!((n.title.as_str(), n.artist.as_str()), ("Hello", "Adele"));
        assert_eq!(n.song.unwrap().artist, "Adele");
    }

    /// Guards: fetching an address the page chose, or building a URL from an
    /// id that is not one.
    #[test]
    fn artwork_is_a_youtube_thumbnail_or_an_https_og_image_only() {
        let mut p = page("www.youtube.com", "T - YouTube");
        p.yt = "../../etc/x".into();
        assert_eq!(now_playing(source(), p).unwrap().artwork_url, None);

        let mut p = page("example.com", "Episode 4");
        p.art = "https://cdn.example.com/c.jpg".into();
        assert_eq!(now_playing(source(), p).unwrap().artwork_url.as_deref(), Some("https://cdn.example.com/c.jpg"));
        let mut p = page("example.com", "Episode 4");
        p.art = "http://cdn.example.com/c.jpg".into();
        assert_eq!(now_playing(source(), p).unwrap().artwork_url, None);
        // No song for a site that is not YouTube.
        assert_eq!(now_playing(source(), page("example.com", "A - B")).unwrap().song, None);
    }

    /// Guards the silent failure: a browser with the setting off answers every
    /// tab with an error and no media, which must read as "turned off", not as
    /// "nothing playing".
    #[test]
    fn js_off_is_told_from_an_idle_browser() {
        let chromium = "Executing JavaScript through AppleScript is turned off. To turn it on, from the menu bar, go to View > Developer > Allow JavaScript from Apple Events.";
        let safari = "You must enable 'Allow JavaScript from Apple Events' in the Developer section of Safari Settings to use 'do JavaScript'.";
        assert!(is_js_off(chromium) && is_js_off(safari));
        assert!(!is_js_off("Tab was closed"));

        for msg in [chromium, safari] {
            let off = json!({ "tabs": [], "errors": [msg], "ok": 0 }).to_string();
            assert_eq!(parse_scan("Brave Browser", &off).err(), Some(IssueKind::JsOff));
        }
        let idle = json!({ "tabs": [], "errors": [], "ok": 9 }).to_string();
        assert!(parse_scan("Brave Browser", &idle).unwrap().is_empty());
        // One window with the setting off beside windows that answered is not "off".
        let mixed = json!({ "tabs": [], "errors": [chromium], "ok": 4 }).to_string();
        assert!(parse_scan("Brave Browser", &mixed).is_ok());
        assert!(parse_scan("Brave Browser", "garbage").unwrap().is_empty());
    }

    #[test]
    fn a_scan_reply_becomes_hits_that_name_their_tab() {
        let reply = json!({ "ok": 3, "errors": [], "tabs": [
            { "w": "7", "t": "42", "page": {
                "host": "www.youtube.com", "title": "A - YouTube", "cur": 3.0, "dur": 100.0,
                "paused": true, "rate": 1.0, "yt": "dQw4w9WgXcQ", "channel": "A" } },
        ]})
        .to_string();
        let hits = parse_scan("Safari", &reply).unwrap();
        let n = &hits[0];
        assert_eq!(n.source.app, "Safari");
        assert_eq!(n.source.tab.as_deref(), Some("7/42"));
        assert_eq!(n.source.host.as_deref(), Some("www.youtube.com"));
        assert_eq!(n.state, PlayState::Paused);
    }

    #[test]
    fn only_browsers_that_are_running_are_found() {
        let out = "412 Safari\n3041 Brave Browser\n";
        assert_eq!(parse_running(out), vec![Browser::Brave, Browser::Safari]);
        assert!(parse_running("").is_empty());
    }

    /// The page script is JavaScript the compiler never sees: parse it.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_page_scripts_are_valid_javascript() {
        for body in [READ_PAGE, CONTROL_PAGE] {
            let src = format!("function run() {{ return typeof ({}); }}", page_fn(body));
            let out = std::process::Command::new("osascript")
                .args(["-l", "JavaScript", "-e", &src])
                .output()
                .unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        }
    }
}
