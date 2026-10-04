//! Video and audio playing in a browser tab (YouTube, Twitch, a podcast
//! page), read and driven through each browser's own scripting: a small
//! script runs *inside* each tab and reports its `<video>`/`<audio>`.
//!
//! No private API. It costs the user one switch per browser, which Aloud can
//! only point at and never flip: Brave/Chrome/Edge › View › Developer ›
//! "Allow JavaScript from Apple Events", Safari › Develop › the same words.
//! Until it is on, every browser reports [`BrowserIssueKind::JsOff`] and the view says
//! so, rather than saying "nothing playing" over a playing video.
//!
//! Privacy: only a tab that holds playing (or part-played) media is reported,
//! and only its title, host, position and the artwork address leave the
//! script. Page text, other tabs' URLs, cookies and forms are never read, and
//! the tab list's URLs are only filtered to http(s) inside osascript.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use aloud_native::{run_args, run_jxa, ScriptError};
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
    pub const ALL: [Browser; 4] = [
        Browser::Brave,
        Browser::Chrome,
        Browser::Edge,
        Browser::Safari,
    ];

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
pub enum BrowserIssueKind {
    /// "Allow JavaScript from Apple Events" is off.
    JsOff,
    /// macOS Automation permission for the browser was refused.
    NotPermitted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BrowserIssue {
    pub app: String,
    pub kind: BrowserIssueKind,
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

/// Asks each tab for its media report and prints one line per tab: `T w t json`
/// for a tab with media, `K` for one that answered with none, `S w/t` for one
/// that did not answer, `E message` for an error. Argv: the page function,
/// the "w/t,w/t" tabs to ask (empty: walk every window), and the ",w/t,"
/// tabs known not to answer.
///
/// AppleScript rather than JXA, for `with timeout`. A tab the browser has
/// discarded to save memory (Chrome's Memory Saver does it to idle tabs, and
/// Brave's does too) never answers `execute javascript`, and without a
/// timeout one such tab holds the whole scan for the Apple Event's two minutes.
/// A discarded tab cannot be playing, so skipping it loses nothing.
///
/// Window and Chromium tab ids are stable; Safari has no tab ids, so its tabs
/// are told apart by position, and the page script's host check catches a tab
/// that has moved. One failing window ends its walk: "JavaScript is off" is
/// per browser, and the rest would fail the same way.
const SCAN: &str = r#"
on run argv
	set code to "(" & (item 1 of argv) & ")(null)"
	set only to item 2 of argv
	set skip to item 3 of argv
	set out to ""
	set hung to 0
	if only is not "" then
		repeat with entry in my splitOn(only, ",")
			set parts to my splitOn(entry, "/")
			set out to out & my probe(item 1 of parts, item 2 of parts, code)
		end repeat
		return out
	end if
	tell application "@APP@" to set winIds to id of every window
	repeat with wid in winIds
		try
			tell application "@APP@" to set urls to URL of every tab of window id wid
			set tabIds to my tabIds(wid, count of urls)
		on error
			set urls to {}
			set tabIds to {}
		end try
		repeat with i from 1 to count of urls
			set u to item i of urls
			set tid to item i of tabIds
			if u is not missing value and (u starts with "http" or u starts with "file:") and skip does not contain ("," & wid & "/" & tid & ",") then
				set got to my probe(wid, tid, code)
				set out to out & got
				if got starts with "S" then set hung to hung + 1
				if got starts with "E" and got contains "Apple Events" then exit repeat
				-- A browser full of discarded tabs would otherwise cost a second each;
				-- the next walk skips these and goes on.
				if hung is 5 then return out
			end if
		end repeat
	end repeat
	return out
end run

on probe(wid, tid, code)
	try
		with timeout of 1 second
			tell application "@APP@" to set r to @RUN@
		end timeout
	on error e number n
		if n is -1712 then return "S" & tab & wid & "/" & tid & linefeed
		return "E" & tab & my oneLine(e) & linefeed
	end try
	try
		if r starts with "{" then return "T" & tab & wid & tab & tid & tab & r & linefeed
	end try
	return "K" & linefeed
end probe

on splitOn(s, d)
	set AppleScript's text item delimiters to d
	set parts to text items of s
	set AppleScript's text item delimiters to ""
	return parts
end splitOn

on oneLine(s)
	set AppleScript's text item delimiters to {linefeed, return}
	set parts to text items of s
	set AppleScript's text item delimiters to " "
	set s to parts as text
	set AppleScript's text item delimiters to ""
	return s
end oneLine
"#;

/// Chromium numbers its tabs; Safari has only positions.
const CHROMIUM_TABS: &str = r#"
on tabIds(wid, n)
	tell application "@APP@" to return id of every tab of window id wid
end tabIds"#;
const SAFARI_TABS: &str = r#"
on tabIds(wid, n)
	set ids to {}
	repeat with i from 1 to n
		set end of ids to i
	end repeat
	return ids
end tabIds"#;
const CHROMIUM_RUN: &str = "execute (tab id tid of window id wid) javascript code";
const SAFARI_RUN: &str = "do JavaScript code in (tab (tid as integer) of window id wid)";

/// The scan script for `app`. The app name is one of ours (`Browser::app`) or
/// the example's argument, never page data.
fn scan_script(app: &str, safari: bool) -> String {
    let (tabs, run) = if safari {
        (SAFARI_TABS, SAFARI_RUN)
    } else {
        (CHROMIUM_TABS, CHROMIUM_RUN)
    };
    format!("{SCAN}\n{tabs}")
        .replace("@RUN@", run)
        .replace("@APP@", app)
}

const CONTROL: &str = r#"
const A = Application(ARG.app);
const w = A.windows.byId(Number(ARG.w));
const tab = ARG.safari ? w.tabs[Number(ARG.t) - 1] : w.tabs.byId(Number(ARG.t));
const code = "(" + ARG.js + ")(" + JSON.stringify(ARG.args) + ")";
return String(ARG.safari ? A.doJavaScript(code, { in: tab }) : tab.execute({ javascript: code }));"#;

/// One scan's answer, from the script's lines.
#[derive(Debug, Default)]
struct Reply {
    tabs: Vec<Tab>,
    errors: Vec<String>,
    /// "window/tab" of the tabs that did not answer in time.
    slow: Vec<String>,
    /// Tabs that answered, with media or without.
    ok: u32,
}

#[derive(Debug)]
struct Tab {
    w: String,
    t: String,
    page: Page,
}

fn parse_lines(out: &str) -> Reply {
    let mut reply = Reply::default();
    for line in out.lines() {
        let mut f = line.splitn(4, '\t');
        match (f.next(), f.next(), f.next(), f.next()) {
            (Some("K"), ..) => reply.ok += 1,
            (Some("S"), Some(id), ..) => reply.slow.push(id.into()),
            (Some("E"), Some(msg), ..) => reply.errors.push(msg.into()),
            (Some("T"), Some(w), Some(t), Some(json)) => {
                reply.ok += 1;
                if let Ok(page) = serde_json::from_str::<Page>(json) {
                    reply.tabs.push(Tab {
                        w: w.into(),
                        t: t.into(),
                        page,
                    });
                }
            }
            _ => {}
        }
    }
    reply
}

/// True for the error a browser gives while "Allow JavaScript from Apple
/// Events" is off. Chromium says "...turned off. ...go to View > Developer >
/// Allow JavaScript from Apple Events"; Safari "You must enable 'Allow
/// JavaScript from Apple Events'". Both carry the setting's name.
pub fn is_js_off(message: &str) -> bool {
    message.contains("Allow JavaScript from Apple Events")
}

/// Reads one scan's lines into hits and the tabs that would not answer, or
/// the issue that stopped it. A scan where no tab answered and one said
/// "JavaScript is off" is `JsOff`; a scan that simply found no media is empty.
fn parse_scan(app: &str, out: &str) -> Result<(Vec<NowPlaying>, Vec<String>), BrowserIssueKind> {
    let reply = parse_lines(out);
    if reply.ok == 0 && reply.errors.iter().any(|e| is_js_off(e)) {
        return Err(BrowserIssueKind::JsOff);
    }
    let hits = reply
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
        .collect();
    Ok((hits, reply.slow))
}

fn now_playing(source: Source, p: Page) -> Option<NowPlaying> {
    let youtube = is_youtube(&p.host);
    let raw = if p.mt.is_empty() { &p.title } else { &p.mt };
    let mut title = clean_title(&p.host, raw);
    if title.is_empty() {
        title.clone_from(&p.host);
    }
    let artist = if p.host == "music.youtube.com" {
        p.mb.split('•')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string()
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
        state: if p.paused {
            PlayState::Paused
        } else {
            PlayState::Playing
        },
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
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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
    let mut found = s.match_indices(" - ").chain(s.match_indices(" \u{2013} "));
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
        "official",
        "lyric",
        "video",
        "audio",
        "visualizer",
        "4k",
        "hd",
        "hq",
    ];
    let mut out = String::new();
    let mut rest = title;
    while let Some(open) = rest.find(['(', '[']) {
        let close_ch = if rest[open..].starts_with('(') {
            ')'
        } else {
            ']'
        };
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
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    let (c, a) = (norm(channel), norm(artist));
    c.len() >= 3 && a.len() >= 3 && (c.contains(&a) || a.contains(&c))
}

// ---- scanning -------------------------------------------------------------

/// A browser's last verdict, so a browser that cannot be read costs one call
/// every few minutes, not one per poll.
static ISSUES: Mutex<Option<HashMap<Browser, (BrowserIssueKind, Instant)>>> = Mutex::new(None);

/// What a browser's tabs looked like at the last full walk.
#[derive(Default)]
struct Walk {
    at: Option<Instant>,
    /// How long the walk took: it grows with the tab count.
    took: Duration,
    /// "window/tab" of the tabs that had media.
    media: Vec<String>,
    /// Tabs that did not answer (discarded), and when each was seen so.
    slow: HashMap<String, Instant>,
}

static WALKS: Mutex<Option<HashMap<Browser, Walk>>> = Mutex::new(None);

/// How long "JavaScript is off" is believed before the browser is asked again.
const RECHECK: Duration = Duration::from_secs(3 * 60);
/// Between full walks only the tabs that had media are asked again. A new
/// video is therefore found within a walk's period, and a playing one is
/// followed every poll for the cost of one tab, however many are open.
const WALK_EVERY: Duration = Duration::from_secs(3);
/// A walk is never more than about one part in this of the time: a 40-tab
/// browser's walk takes ~0.9 s, so it is repeated every ~4.5 s, not every 3.
const WALK_DUTY: u32 = 5;
/// A tab that did not answer is left alone this long. It is usually discarded,
/// and a discarded tab is not playing; one the user brings back is found when
/// this runs out.
const SLOW_FOR: Duration = Duration::from_secs(10 * 60);

fn lock<T>(
    m: &'static Mutex<Option<HashMap<Browser, T>>>,
) -> std::sync::MutexGuard<'static, Option<HashMap<Browser, T>>> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Forgets every verdict, for "I turned it on" in the view.
pub fn recheck() {
    *lock(&ISSUES) = None;
    *lock(&WALKS) = None;
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
pub fn scan() -> (Vec<NowPlaying>, Vec<BrowserIssue>) {
    let (mut hits, mut issues) = (Vec::new(), Vec::new());
    let running = running();
    // A browser that quit and came back has a fresh setting and permission.
    lock(&ISSUES)
        .get_or_insert_default()
        .retain(|b, _| running.contains(b));
    lock(&WALKS)
        .get_or_insert_default()
        .retain(|b, _| running.contains(b));
    for browser in running {
        let known = lock(&ISSUES).get_or_insert_default().get(&browser).copied();
        if let Some((kind, _)) = known.filter(|(_, at)| at.elapsed() < RECHECK) {
            issues.push(BrowserIssue {
                app: browser.app().into(),
                kind,
            });
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
                issues.push(BrowserIssue {
                    app: browser.app().into(),
                    kind,
                });
            }
        }
    }
    (hits, issues)
}

/// ponytail: the scan runs on the caller's thread, so a browser restored with
/// dozens of tabs it has not loaded (they never answer) delays a poll by up to
/// five seconds per walk until each has been noted and skipped. The upgrade is
/// a scanner thread that publishes its last result.
fn scan_browser(browser: Browser) -> Result<Vec<NowPlaying>, BrowserIssueKind> {
    let (only, skip) = {
        let mut walks = lock(&WALKS);
        let walk = walks.get_or_insert_default().entry(browser).or_default();
        walk.slow.retain(|_, at| at.elapsed() < SLOW_FOR);
        let due = walk
            .at
            .is_none_or(|at| at.elapsed() >= WALK_EVERY.max(walk.took * WALK_DUTY));
        let skip: Vec<String> = walk.slow.keys().cloned().collect();
        let only = (!due && !walk.media.is_empty()).then(|| {
            walk.media
                .iter()
                .filter(|t| !walk.slow.contains_key(*t))
                .cloned()
                .collect::<Vec<_>>()
        });
        (only, skip)
    };
    let started = Instant::now();
    let (hits, slow) = scan_app(browser.app(), browser.safari(), only.as_deref(), &skip)?;
    let mut walks = lock(&WALKS);
    let walk = walks.get_or_insert_default().entry(browser).or_default();
    walk.slow
        .extend(slow.into_iter().map(|t| (t, Instant::now())));
    if only.is_none() {
        walk.at = Some(Instant::now());
        // Without the timeouts, which are not the cost of a tab that answers.
        walk.took = started
            .elapsed()
            .saturating_sub(Duration::from_secs(walk.slow.len() as u64));
        walk.media = hits.iter().filter_map(|n| n.source.tab.clone()).collect();
    }
    Ok(hits)
}

/// One osascript call over `app`'s tabs (`only`: "window/tab" ids, else every
/// tab but `skip`). Returns the media tabs and the tabs that did not answer.
/// Public for the example, which scans a browser by any name.
pub fn scan_app(
    app: &str,
    safari: bool,
    only: Option<&[String]>,
    skip: &[String],
) -> Result<(Vec<NowPlaying>, Vec<String>), BrowserIssueKind> {
    let only = only.map(|ids| ids.join(",")).unwrap_or_default();
    let skip = format!(",{},", skip.join(","));
    let js = page_fn(READ_PAGE);
    match run_args(app, &scan_script(app, safari), &[&js, &only, &skip]) {
        Ok(out) => parse_scan(app, &out),
        Err(ScriptError::NotPermitted { .. }) => Err(BrowserIssueKind::NotPermitted),
        Err(e) => {
            tracing::debug!(%e, app, "browser media scan failed");
            Ok((Vec::new(), Vec::new()))
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
    let (w, t) = source
        .tab
        .as_deref()
        .and_then(|id| id.split_once('/'))
        .ok_or_else(|| failed("no tab to control"))?;
    let arg = json!({
        "app": source.app, "safari": source.app == Browser::Safari.app(), "w": w, "t": t,
        "js": page_fn(CONTROL_PAGE),
        "args": cmd.args(source.host.as_deref().unwrap_or_default()),
    });
    match run_jxa(&source.app, CONTROL, &arg)?.as_str() {
        "ok" => Ok(()),
        "unsupported" => Err(failed("this page has no next button")),
        _ => Err(failed(
            "that tab changed; the next reading will find the video",
        )),
    }
}

/// Brings `app` forward so the user can reach its menu bar. Only a browser
/// Aloud scans, so this cannot be turned into "open anything".
pub fn open(app: &str) {
    #[cfg(target_os = "macos")]
    if Browser::from_app(app).is_some() {
        let _ = std::process::Command::new("open")
            .args(["-a", app])
            .status();
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
        Source {
            app: "Brave Browser".into(),
            host: None,
            tab: Some("1/2".into()),
        }
    }

    /// Guards: the notification counter and site suffix showing up as part of
    /// the song ("(3) Song - YouTube" under the artwork).
    #[test]
    fn youtube_titles_lose_the_counter_and_the_suffix() {
        let yt = "www.youtube.com";
        assert_eq!(
            clean_title(yt, "(3) Never Gonna Give You Up - YouTube"),
            "Never Gonna Give You Up"
        );
        assert_eq!(clean_title(yt, "(99+) A (Live) - YouTube"), "A (Live)");
        assert_eq!(
            clean_title("music.youtube.com", "Song - YouTube Music"),
            "Song"
        );
        assert_eq!(clean_title(yt, "Plain"), "Plain");
        // Other sites keep their own punctuation.
        assert_eq!(
            clean_title("example.com", "(3) Inbox - YouTube"),
            "(3) Inbox - YouTube"
        );
    }

    #[test]
    fn a_youtube_page_becomes_a_track_with_a_thumbnail_and_speed() {
        let mut p = page(
            "www.youtube.com",
            "(2) Rick Astley - Never Gonna Give You Up (Official Video) - YouTube",
        );
        p.channel = "Rick Astley".into();
        let n = now_playing(source(), p).unwrap();
        assert_eq!(
            n.title,
            "Rick Astley - Never Gonna Give You Up (Official Video)"
        );
        assert_eq!(n.artist, "Rick Astley");
        assert_eq!(
            n.artwork_url.as_deref(),
            Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg")
        );
        assert_eq!(
            (n.position_ms, n.duration_ms, n.speed_pct),
            (12_500, 213_000, 125)
        );
        assert_eq!(n.state, PlayState::Playing);
        let song = n.song.unwrap();
        assert_eq!(
            (song.artist.as_str(), song.title.as_str()),
            ("Rick Astley", "Never Gonna Give You Up")
        );
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
        assert_eq!(
            now_playing(source(), p).unwrap().artwork_url.as_deref(),
            Some("https://cdn.example.com/c.jpg")
        );
        let mut p = page("example.com", "Episode 4");
        p.art = "http://cdn.example.com/c.jpg".into();
        assert_eq!(now_playing(source(), p).unwrap().artwork_url, None);
        // No song for a site that is not YouTube.
        assert_eq!(
            now_playing(source(), page("example.com", "A - B"))
                .unwrap()
                .song,
            None
        );
    }

    /// Guards the silent failure: a browser with the setting off answers every
    /// tab with an error and no media, which must read as "turned off", not as
    /// "nothing playing".
    #[test]
    fn js_off_is_told_from_an_idle_browser() {
        let chromium = "Brave Browser got an error: Executing JavaScript through AppleScript is turned off. To turn it on, from the menu bar, go to View > Developer > Allow JavaScript from Apple Events. (12)";
        let safari = "Safari got an error: You must enable 'Allow JavaScript from Apple Events' in the Developer section of Safari Settings to use 'do JavaScript'. (8)";
        assert!(is_js_off(chromium) && is_js_off(safari));
        assert!(!is_js_off("Tab was closed"));

        for msg in [chromium, safari] {
            let off = format!("E\t{msg}\n");
            assert_eq!(
                parse_scan("Brave Browser", &off).err(),
                Some(BrowserIssueKind::JsOff)
            );
        }
        let idle = "K\nK\nK\n";
        assert!(parse_scan("Brave Browser", idle).unwrap().0.is_empty());
        // One window with the setting off beside windows that answered is not "off".
        let mixed = format!("K\nE\t{chromium}\nK\n");
        assert!(parse_scan("Brave Browser", &mixed).is_ok());
        assert!(parse_scan("Brave Browser", "garbage").unwrap().0.is_empty());
        assert!(parse_scan("Brave Browser", "").unwrap().0.is_empty());
    }

    #[test]
    fn a_scan_reply_becomes_hits_that_name_their_tab() {
        let page = r#"{"host":"www.youtube.com","title":"A - YouTube","cur":3.0,"dur":100.0,"paused":true,"rate":1.0,"yt":"dQw4w9WgXcQ","channel":"A"}"#;
        let reply = format!("K\nT\t7\t42\t{page}\nS\t7/43\nT\t7\t44\tnot json\n");
        let (hits, slow) = parse_scan("Safari", &reply).unwrap();
        assert_eq!(hits.len(), 1);
        let n = &hits[0];
        assert_eq!(n.source.app, "Safari");
        assert_eq!(n.source.tab.as_deref(), Some("7/42"));
        assert_eq!(n.source.host.as_deref(), Some("www.youtube.com"));
        assert_eq!(n.state, PlayState::Paused);
        // A tab that did not answer is remembered, not mistaken for "off".
        assert_eq!(slow, vec!["7/43".to_string()]);
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
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    /// The scan script is AppleScript the compiler never sees. It needs the
    /// browser's dictionary to compile, so it checks the browsers installed here.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_scan_scripts_compile() {
        for b in Browser::ALL {
            if !std::path::Path::new(&format!("/Applications/{}.app", b.app())).exists() {
                continue;
            }
            let out = std::process::Command::new("osacompile")
                .args(["-o", "/dev/null", "-e", &scan_script(b.app(), b.safari())])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}: {}",
                b.app(),
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}
