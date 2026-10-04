//! Now Playing from any app (Podcasts, VLC, a browser, ...) with no setup, from
//! macOS's private MediaRemote framework.
//!
//! PRIVATE API, and it can break on any macOS update. From macOS 15.4 only
//! Apple-signed processes may call MediaRemote, so this runs the vendored
//! `mediaremote-adapter` (`crates/media/mediaremote`, BSD-3): `/usr/bin/perl`,
//! which Apple signs, loads a small dylib and streams the now-playing state as
//! JSON lines. One long-lived child streams; a command ("pause", "seek") is a
//! second, short-lived one, because the adapter has no way to take it on stdin.
//!
//! Nothing here is required: while the helper is off, missing or broken,
//! [`current`] is None and `now_playing` falls back to Spotify, Music and the
//! browser scan. It reads only what is playing: title, artist, album, cover,
//! position and the app's bundle id.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Condvar, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aloud_native::ScriptError;
use base64::Engine;
use serde_json::{Map, Value};

use crate::artwork::data_uri;
use crate::browser::Browser;
use crate::now_playing::{NowPlaying, PlayState, Player, Song, Source, Transport};

const PERL: &str = "/usr/bin/perl";
const SCRIPT: &str = "mediaremote-adapter.pl";
const FRAMEWORK: &str = "MediaRemoteAdapter.framework";

/// A helper that stayed up this long was working; its next death starts the
/// backoff over.
const HEALTHY_AFTER: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(300);
/// How long something else may play while MediaRemote reports nothing before
/// it is called broken.
const SILENT_FOR: Duration = Duration::from_secs(30);
/// A command that takes longer than this is hung, not slow.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// Where the helper is: the folder `crates/media/mediaremote/build.sh` writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    script: PathBuf,
    framework: PathBuf,
}

impl Paths {
    fn in_dir(dir: &Path) -> Option<Paths> {
        let paths = Paths {
            script: dir.join(SCRIPT),
            framework: dir.join(FRAMEWORK),
        };
        let built = paths.script.is_file() && paths.framework.join("MediaRemoteAdapter").is_file();
        built.then_some(paths)
    }

    /// `ALOUD_MEDIAREMOTE_DIR`, else the copy staged into the app bundle
    /// (`Contents/Resources/resources/mediaremote`), else the one built into the
    /// repo's `resources/` in development, the way the engine's `axd` is found.
    pub fn locate() -> Option<Paths> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        [
            std::env::var_os("ALOUD_MEDIAREMOTE_DIR").map(PathBuf::from),
            exe_dir.map(|d| d.join("../Resources/resources/mediaremote")),
            Some(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../apps/desktop/src-tauri/resources/mediaremote"),
            ),
        ]
        .into_iter()
        .flatten()
        .find_map(|dir| Paths::in_dir(&dir))
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(PERL);
        cmd.arg(&self.script).arg(&self.framework).args(args);
        cmd
    }
}

/// One line of `stream`: either the whole state, or what changed since the
/// last line (a null value removes a key). An empty whole state is "nothing
/// playing".
#[derive(Debug, PartialEq)]
struct Frame {
    diff: bool,
    fields: Map<String, Value>,
    /// The cover, decoded and checked: `Some(None)` is "the cover went away",
    /// None is "this line says nothing about the cover".
    art: Option<Option<String>>,
}

impl Frame {
    fn parse(line: &str) -> Option<Frame> {
        let mut v: Value = serde_json::from_str(line).ok()?;
        if v["type"] != "data" {
            return None;
        }
        let diff = v["diff"].as_bool().unwrap_or(false);
        let Value::Object(mut fields) = v["payload"].take() else {
            return None;
        };
        // Decoded here, on the reader thread and outside the lock: a cover is
        // hundreds of KB of base64.
        let art = fields.remove("artworkData").map(|a| {
            a.as_str()
                .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
                .and_then(|bytes| data_uri(&bytes))
        });
        Some(Frame { diff, fields, art })
    }
}

/// The helper's state, built up from its frames.
#[derive(Debug, Default)]
struct Session {
    fields: Map<String, Value>,
    art: Option<String>,
    /// Bumped whenever the cover changes, so a view can tell a cover arrived
    /// late (they often do, a moment after the title).
    art_rev: u32,
}

impl Session {
    fn merge(&mut self, frame: Frame) {
        if !frame.diff {
            self.fields.clear();
            // A new item has no cover until its own arrives.
            if self.art.take().is_some() {
                self.art_rev += 1;
            }
        }
        for (key, value) in frame.fields {
            if value.is_null() {
                self.fields.remove(&key);
            } else {
                self.fields.insert(key, value);
            }
        }
        if let Some(art) = frame.art {
            self.art = art;
            self.art_rev += 1;
        }
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.fields
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    }

    /// The app to show: a helper process (a browser's, say) names the app that
    /// owns it as its parent.
    fn bundle(&self) -> Option<&str> {
        self.text("parentApplicationBundleIdentifier")
            .or_else(|| self.text("bundleIdentifier"))
    }

    /// Who is playing: the bundle id, or for a process that has none (mpv,
    /// ffplay, a script) `pid:<n>`.
    fn app_id(&self) -> Option<String> {
        self.bundle().map(str::to_string).or_else(|| {
            let pid = self.fields.get("processIdentifier")?.as_u64()?;
            Some(format!("pid:{pid}"))
        })
    }

    /// What changes what the notch shows, as opposed to the clock moving.
    fn signature(&self) -> (Option<String>, Option<String>, bool, u32) {
        (
            self.app_id(),
            self.text("title").map(str::to_string),
            self.fields.get("playing").and_then(Value::as_bool) == Some(true),
            self.art_rev,
        )
    }

    /// The track as of `now_micros` (since the epoch): the adapter reports the
    /// position at a timestamp, and a playing track has moved on since.
    fn now_playing(&self, now_micros: f64, app: String) -> Option<NowPlaying> {
        self.app_id()?;
        let bundle = self.bundle().map(str::to_string);
        let title = self.text("title")?.to_string();
        let num = |key: &str| self.fields.get(key).and_then(Value::as_f64);
        let playing = self.fields.get("playing").and_then(Value::as_bool) == Some(true);
        let rate = num("playbackRate").filter(|r| *r > 0.0).unwrap_or(1.0);
        let duration = num("durationMicros").unwrap_or(0.0).max(0.0);
        let mut position = num("elapsedTimeMicros").unwrap_or(0.0).max(0.0);
        if let (true, Some(at)) = (playing, num("timestampEpochMicros")) {
            position += (now_micros - at).max(0.0) * rate;
        }
        if duration > 0.0 {
            position = position.min(duration);
        }
        let text = |key| self.text(key).unwrap_or_default().to_string();
        let (artist, album) = (text("artist"), text("album"));
        let music = bundle.as_deref().is_some_and(is_music_app);
        let song = (music && !artist.is_empty()).then(|| Song {
            title: title.clone(),
            artist: artist.clone(),
        });
        Some(NowPlaying {
            player: Player::MediaRemote,
            source: Source {
                app,
                bundle_id: bundle,
                host: None,
                tab: None,
            },
            state: if playing {
                PlayState::Playing
            } else {
                PlayState::Paused
            },
            title,
            artist,
            album,
            duration_ms: (duration / 1000.0) as u32,
            position_ms: (position / 1000.0) as u32,
            speed_pct: if playing {
                (rate * 100.0).round() as u16
            } else {
                100
            },
            artwork_url: self
                .art
                .is_some()
                .then(|| format!("mediaremote:{}", self.art_rev)),
            song,
        })
    }
}

/// Apps whose title and artist are a track, so lyrics can be asked for. A
/// browser's title is a page's, a podcast's an episode's, VLC's a file name.
/// ponytail: a list. The adapter also reports `isMusicApp`, but not for every
/// player, and a wrong lyrics lookup is worse than none.
pub fn is_music_app(bundle: &str) -> bool {
    matches!(
        bundle,
        "com.spotify.client"
            | "com.apple.Music"
            | "com.apple.iTunes"
            | "com.tidal.desktop"
            | "com.deezer.deezer-desktop"
            | "com.amazon.music"
    )
}

/// The bundle id of the app a fallback reading came from, so a track that
/// MediaRemote already reports is not shown twice.
fn bundle_of(n: &NowPlaying) -> Option<&str> {
    match n.player {
        Player::Spotify => Some("com.spotify.client"),
        Player::Music => Some("com.apple.Music"),
        Player::Browser => Browser::from_app(&n.source.app).map(Browser::bundle_id),
        Player::MediaRemote => n.source.bundle_id.as_deref(),
    }
}

/// Whether `other` is the same app MediaRemote `mr` reported.
pub fn same_app(mr: &NowPlaying, other: &NowPlaying) -> bool {
    bundle_of(mr).is_some() && bundle_of(mr) == bundle_of(other)
}

/// The helper is run again after it dies, later each time, so a macOS that
/// refuses it costs a spawn every five minutes and not a hot loop.
#[derive(Debug, Default)]
struct Backoff {
    failures: u32,
}

impl Backoff {
    /// How long to wait after a run that lasted `ran`.
    fn after(&mut self, ran: Duration) -> Duration {
        if ran >= HEALTHY_AFTER {
            self.failures = 0;
        }
        let delay = Duration::from_secs(1u64 << self.failures.min(10)).min(MAX_BACKOFF);
        self.failures += 1;
        delay
    }
}

/// Notices MediaRemote saying nothing for [`SILENT_FOR`] while something else
/// plays, which is what a macOS update that closes the door looks like from
/// outside: the helper runs, and no data comes. Reports once per episode.
#[derive(Debug, Default)]
struct SilenceWatch {
    since: Option<Instant>,
    reported: bool,
}

impl SilenceWatch {
    fn observe(&mut self, now: Instant, silent: bool) -> bool {
        if !silent {
            *self = SilenceWatch::default();
            return false;
        }
        let since = *self.since.get_or_insert(now);
        let report = !self.reported && now.duration_since(since) >= SILENT_FOR;
        self.reported |= report;
        report
    }
}

#[derive(Default)]
struct Inner {
    enabled: bool,
    /// Set at exit: the supervisor must not start another child.
    shut: bool,
    paths: Option<Paths>,
    /// The streaming child, kept here so a switch can kill it.
    child: Option<Child>,
    /// Frames have arrived from this child, so it is working.
    live: bool,
    session: Session,
    silence: SilenceWatch,
    supervisor: bool,
    on_change: Option<fn()>,
}

struct Hub {
    inner: Mutex<Inner>,
    wake: Condvar,
}

static HUB: LazyLock<Hub> = LazyLock::new(|| Hub {
    inner: Mutex::default(),
    wake: Condvar::new(),
});

fn lock() -> MutexGuard<'static, Inner> {
    HUB.inner.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Locates the helper and starts the thread that keeps it running. `changed`
/// is called when what is playing changes, so the notch need not wait for its
/// next poll. Harmless to call twice.
pub fn start(changed: fn()) {
    let mut g = lock();
    g.on_change = Some(changed);
    if g.supervisor {
        return;
    }
    g.paths = Paths::locate();
    if g.paths.is_none() {
        tracing::info!("mediaremote: helper not built, using Spotify, Music and browsers only");
        return;
    }
    g.supervisor = true;
    drop(g);
    std::thread::Builder::new()
        .name("mediaremote".into())
        .spawn(supervise)
        .map_or_else(|e| tracing::warn!(%e, "mediaremote: no thread"), drop);
}

/// Runs the helper (`true`) or stops it and forgets what it said (`false`).
pub fn set_enabled(on: bool) {
    let mut g = lock();
    if g.enabled == on {
        return;
    }
    g.enabled = on;
    if !on {
        stop(&mut g);
    }
    HUB.wake.notify_all();
}

/// Kills the helper for good: the app is exiting.
pub fn shutdown() {
    let mut g = lock();
    g.shut = true;
    g.enabled = false;
    stop(&mut g);
    HUB.wake.notify_all();
}

fn stop(g: &mut Inner) {
    if let Some(child) = g.child.as_mut() {
        let _ = child.kill();
    }
    g.live = false;
    g.session = Session::default();
}

/// The helper is up and has reported at least once.
pub fn working() -> bool {
    let g = lock();
    g.enabled && g.live
}

/// What MediaRemote says is playing, or None when it says nothing or is off.
pub fn current() -> Option<NowPlaying> {
    let id = {
        let g = lock();
        if !g.enabled {
            return None;
        }
        g.session.app_id()?
    };
    let app = app_name(&id);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_micros() as f64);
    lock().session.now_playing(now, app)
}

/// The cover of `now`, if it is still the one the helper holds.
pub fn artwork(now: &NowPlaying) -> Option<String> {
    let g = lock();
    let current = format!("mediaremote:{}", g.session.art_rev);
    (now.artwork_url.as_deref() == Some(&current))
        .then(|| g.session.art.clone())
        .flatten()
}

/// Tells the watch whether another source is playing while MediaRemote reports
/// nothing. Logs once when that has gone on too long.
pub fn observe(other_playing: Option<&NowPlaying>, mr: Option<&NowPlaying>) {
    let mut g = lock();
    if !g.enabled || !g.live {
        return;
    }
    let silent = mr.is_none() && other_playing.is_some();
    if g.silence.observe(Instant::now(), silent) {
        tracing::warn!(
            app = other_playing.map(|n| n.source.app.as_str()),
            "mediaremote: nothing reported while another app plays; a macOS update may have closed this private interface"
        );
    }
}

/// The adapter's command ids (`MRCommand`): 0 play, 1 pause, 2 toggle, 4 next,
/// 5 previous.
fn command_id(t: Transport) -> u8 {
    match t {
        Transport::Play => 0,
        Transport::Pause => 1,
        Transport::Toggle => 2,
        Transport::Next => 4,
        Transport::Previous => 5,
    }
}

pub fn send(t: Transport) -> Result<(), ScriptError> {
    run(&["send".into(), command_id(t).to_string()])
}

pub fn seek(position_ms: u32) -> Result<(), ScriptError> {
    run(&["seek".into(), (u64::from(position_ms) * 1000).to_string()])
}

fn run(args: &[String]) -> Result<(), ScriptError> {
    let paths = {
        let g = lock();
        if !g.enabled || !g.live {
            return Err(ScriptError::Failed(
                "Now Playing helper is not running".into(),
            ));
        }
        g.paths.clone()
    }
    .ok_or_else(|| ScriptError::Failed("Now Playing helper is missing".into()))?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut child = paths
        .command(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ScriptError::Failed(e.to_string()))?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ScriptError::Failed("Now Playing helper timed out".into()));
            }
        }
    };
    if status.success() {
        return Ok(());
    }
    Err(ScriptError::Failed(stderr_of(&mut child)))
}

fn stderr_of(child: &mut Child) -> String {
    let mut text = String::new();
    if let Some(err) = child.stderr.take() {
        let _ = err.take(4096).read_to_string(&mut text);
    }
    let text = text.trim();
    if text.is_empty() {
        "Now Playing helper failed".into()
    } else {
        text.lines().last().unwrap_or(text).to_string()
    }
}

/// The app's name, for the badge and its icon. `lsappinfo` answers for a
/// running app, which a playing one is; a miss is not remembered, and neither
/// is a process's name (pids are reused).
fn app_name(bundle: &str) -> String {
    if let Some(pid) = bundle.strip_prefix("pid:") {
        return Command::new("ps")
            .args(["-p", pid, "-o", "comm="])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|out| out.trim().rsplit('/').next().map(str::to_string))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Unknown app".into());
    }
    static NAMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
    if let Some(name) = NAMES
        .lock()
        .ok()
        .and_then(|n| n.as_ref().and_then(|n| n.get(bundle).cloned()))
    {
        return name;
    }
    let looked_up = Command::new("lsappinfo")
        .args(["info", "-only", "name", "-app", bundle])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|out| parse_lsappinfo_name(&out));
    match looked_up {
        Some(name) => {
            if let Ok(mut names) = NAMES.lock() {
                names
                    .get_or_insert_default()
                    .insert(bundle.to_string(), name.clone());
            }
            name
        }
        None => bundle.rsplit('.').next().unwrap_or(bundle).to_string(),
    }
}

/// `"LSDisplayName"="Brave Browser"` is the name.
fn parse_lsappinfo_name(out: &str) -> Option<String> {
    let name = out.trim().split_once('=')?.1.trim().trim_matches('"');
    (!name.is_empty() && name != "[ NULL ]").then(|| name.to_string())
}

fn supervise() {
    let mut backoff = Backoff::default();
    let mut reported = false;
    loop {
        let paths = {
            let mut g = lock();
            while !g.enabled || g.shut {
                if g.shut {
                    return;
                }
                g = HUB.wake.wait(g).unwrap_or_else(PoisonError::into_inner);
            }
            g.paths.clone()
        };
        let Some(paths) = paths else { return };
        let started = Instant::now();
        let outcome = stream(&paths);
        let ran = started.elapsed();

        let mut g = lock();
        g.live = false;
        g.session = Session::default();
        if !g.enabled || g.shut {
            continue;
        }
        let delay = backoff.after(ran);
        if ran >= HEALTHY_AFTER {
            reported = false;
        }
        // Once per outage at warn: a helper that cannot run will say so every
        // time, and the log is not for repeating it.
        if reported {
            tracing::debug!(?outcome, ?delay, "mediaremote: helper stopped");
        } else {
            reported = true;
            tracing::warn!(?outcome, ?delay, "mediaremote: helper stopped, will retry");
        }
        let _ = HUB
            .wake
            .wait_timeout_while(g, delay, |i| i.enabled && !i.shut)
            .unwrap_or_else(PoisonError::into_inner);
    }
}

/// Runs one streaming child to its end and says how it ended.
fn stream(paths: &Paths) -> String {
    // --micros: exact timestamps; --debounce: a track change is several
    // updates in a few milliseconds, and one of them is enough.
    let spawned = paths
        .command(&["stream", "--micros", "--debounce=100"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => return format!("could not start: {e}"),
    };
    let (Some(stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        return "no pipes".into();
    };
    {
        let mut g = lock();
        if !g.enabled || g.shut {
            let _ = child.kill();
            let _ = child.wait();
            return "switched off".into();
        }
        g.child = Some(child);
    }
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Some(frame) = Frame::parse(&line) else {
            continue;
        };
        let mut g = lock();
        if !g.enabled {
            break;
        }
        let before = g.session.signature();
        g.session.merge(frame);
        g.live = true;
        if before != g.session.signature() {
            if let Some(changed) = g.on_change {
                drop(g);
                changed();
            }
        }
    }
    let mut message = String::new();
    let _ = stderr.by_ref().take(4096).read_to_string(&mut message);
    let status = lock().child.take().map(|mut c| {
        let _ = c.kill();
        c.wait()
    });
    format!(
        "exited ({}){}",
        status.map_or("?".into(), |s| format!("{s:?}")),
        message
            .trim()
            .lines()
            .last()
            .map_or(String::new(), |l| format!(": {l}")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(lines: &[&str]) -> Session {
        let mut s = Session::default();
        for line in lines {
            s.merge(Frame::parse(line).unwrap());
        }
        s
    }

    const FULL: &str = r#"{"type":"data","diff":false,"payload":{"bundleIdentifier":"com.spotify.client","playing":true,"title":"Song","artist":"Band","album":"LP","durationMicros":200000000,"elapsedTimeMicros":10000000,"timestampEpochMicros":1000000000,"playbackRate":1}}"#;

    #[test]
    fn a_full_frame_is_a_track() {
        let n = session(&[FULL])
            .now_playing(1_000_000_000.0, "Spotify".into())
            .unwrap();
        assert_eq!(n.player, Player::MediaRemote);
        assert_eq!(n.source.bundle_id.as_deref(), Some("com.spotify.client"));
        assert_eq!((n.title.as_str(), n.artist.as_str()), ("Song", "Band"));
        assert_eq!((n.duration_ms, n.position_ms), (200_000, 10_000));
        assert_eq!(n.state, PlayState::Playing);
        // A music app names a song, so lyrics may be asked for.
        assert_eq!(n.lyrics_query(), Some(("Song", "Band")));
    }

    /// Guards: a position frozen at the last update; the adapter reports it at
    /// a timestamp and a playing track has moved on since.
    #[test]
    fn a_playing_track_moves_on_and_a_paused_one_does_not() {
        let mut s = session(&[FULL]);
        let later = 1_000_000_000.0 + 5_000_000.0;
        assert_eq!(
            s.now_playing(later, "S".into()).unwrap().position_ms,
            15_000
        );
        s.merge(
            Frame::parse(r#"{"type":"data","diff":true,"payload":{"playing":false}}"#).unwrap(),
        );
        assert_eq!(
            s.now_playing(later, "S".into()).unwrap().position_ms,
            10_000
        );
        // Never past the end, which a late read could outrun.
        let mut s = session(&[FULL]);
        s.merge(
            Frame::parse(
                r#"{"type":"data","diff":true,"payload":{"elapsedTimeMicros":199000000}}"#,
            )
            .unwrap(),
        );
        assert_eq!(
            s.now_playing(later, "S".into()).unwrap().position_ms,
            200_000
        );
    }

    #[test]
    fn a_diff_changes_only_what_it_names_and_null_removes() {
        let mut s = session(&[FULL]);
        s.merge(
            Frame::parse(r#"{"type":"data","diff":true,"payload":{"title":"Next","album":null}}"#)
                .unwrap(),
        );
        let n = s.now_playing(0.0, "S".into()).unwrap();
        assert_eq!(
            (n.title.as_str(), n.artist.as_str(), n.album.as_str()),
            ("Next", "Band", "")
        );
    }

    /// Guards: the empty full frame being read as a track, or ignored so that
    /// the last track stays up after the player quits.
    #[test]
    fn an_empty_full_frame_is_nothing_playing() {
        let mut s = session(&[FULL]);
        s.merge(Frame::parse(r#"{"type":"data","diff":false,"payload":{}}"#).unwrap());
        assert_eq!(s.now_playing(0.0, "S".into()), None);
    }

    /// Guards: a track with no title being shown as an empty row, and lines
    /// that are not frames killing the stream.
    /// Guards: a player with no bundle (mpv, ffplay) vanishing from the notch.
    #[test]
    fn a_process_without_a_bundle_is_known_by_its_pid() {
        let s = session(&[
            r#"{"type":"data","diff":false,"payload":{"processIdentifier":4242,"playing":true,"title":"clip"}}"#,
        ]);
        assert_eq!(s.app_id().as_deref(), Some("pid:4242"));
        let n = s.now_playing(0.0, "mpv".into()).unwrap();
        assert_eq!((n.source.app.as_str(), n.source.bundle_id), ("mpv", None));
    }

    #[test]
    fn untitled_and_unparsable_lines_are_not_tracks() {
        let s = session(&[
            r#"{"type":"data","diff":false,"payload":{"bundleIdentifier":"org.videolan.vlc","playing":true}}"#,
        ]);
        assert_eq!(s.now_playing(0.0, "VLC".into()), None);
        assert_eq!(Frame::parse("not json"), None);
        assert_eq!(Frame::parse(r#"{"type":"other"}"#), None);
        assert_eq!(Frame::parse(r#"{"type":"data","diff":false}"#), None);
    }

    /// Guards: a browser's title being sent to the lyrics service as a song,
    /// and a parent app's name being lost to its helper process.
    #[test]
    fn only_music_apps_name_a_song_and_a_parent_app_wins() {
        let s = session(&[
            r#"{"type":"data","diff":false,"payload":{"bundleIdentifier":"com.brave.Browser.helper","parentApplicationBundleIdentifier":"com.brave.Browser","playing":true,"title":"We Pitched Ideas","artist":"Family Friendly"}}"#,
        ]);
        let n = s.now_playing(0.0, "Brave Browser".into()).unwrap();
        assert_eq!(n.source.bundle_id.as_deref(), Some("com.brave.Browser"));
        assert_eq!(n.song, None);
        assert_eq!(n.lyrics_query(), None);
    }

    /// Guards: a cover that arrives after the title never being shown, and one
    /// track's cover sitting under the next track.
    #[test]
    fn a_late_cover_bumps_the_revision_and_a_new_track_drops_it() {
        let png = base64::engine::general_purpose::STANDARD.encode([0x89, b'P', b'N', b'G', 1, 2]);
        let mut s = session(&[FULL]);
        let before = s.now_playing(0.0, "S".into()).unwrap();
        assert_eq!(before.artwork_url, None);
        s.merge(
            Frame::parse(&format!(
                r#"{{"type":"data","diff":true,"payload":{{"artworkData":"{png}"}}}}"#
            ))
            .unwrap(),
        );
        let with = s.now_playing(0.0, "S".into()).unwrap();
        assert_ne!(with.artwork_url, None);
        assert!(s
            .art
            .as_deref()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        s.merge(Frame::parse(FULL).unwrap());
        assert_eq!(s.now_playing(0.0, "S".into()).unwrap().artwork_url, None);
        assert!(s.art_rev > 1);
        // Bytes that are not an image are no cover.
        s.merge(
            Frame::parse(r#"{"type":"data","diff":true,"payload":{"artworkData":"aGVsbG8="}}"#)
                .unwrap(),
        );
        assert_eq!(s.art, None);
    }

    /// Guards: a helper that dies instantly being respawned in a hot loop, and
    /// a day of good running not earning the short delay back.
    #[test]
    fn restarts_back_off_and_a_healthy_run_resets_them() {
        let mut b = Backoff::default();
        let quick = Duration::from_millis(200);
        let secs: Vec<u64> = (0..12).map(|_| b.after(quick).as_secs()).collect();
        assert_eq!(&secs[..5], &[1, 2, 4, 8, 16]);
        assert_eq!(secs[11], 300);
        assert_eq!(b.after(HEALTHY_AFTER).as_secs(), 1);
        assert_eq!(b.after(quick).as_secs(), 2);
    }

    /// Guards: calling MediaRemote broken because a player was merely idle for
    /// a moment, and warning on every poll once it is.
    #[test]
    fn silence_is_reported_once_after_the_wait() {
        let mut w = SilenceWatch::default();
        let t0 = Instant::now();
        assert!(!w.observe(t0, true));
        assert!(!w.observe(t0 + SILENT_FOR - Duration::from_secs(1), true));
        assert!(w.observe(t0 + SILENT_FOR, true));
        assert!(!w.observe(t0 + SILENT_FOR * 2, true));
        // It recovered, then went quiet again: a new episode.
        assert!(!w.observe(t0 + SILENT_FOR * 3, false));
        assert!(!w.observe(t0 + SILENT_FOR * 4, true));
        assert!(w.observe(t0 + SILENT_FOR * 5, true));
    }

    #[test]
    fn transport_maps_to_the_adapters_command_ids() {
        let ids: Vec<u8> = [
            Transport::Play,
            Transport::Pause,
            Transport::Toggle,
            Transport::Next,
            Transport::Previous,
        ]
        .map(command_id)
        .to_vec();
        assert_eq!(ids, [0, 1, 2, 4, 5]);
    }

    #[test]
    fn lsappinfo_answers_parse() {
        assert_eq!(
            parse_lsappinfo_name("\"LSDisplayName\"=\"Brave Browser\"\n").as_deref(),
            Some("Brave Browser")
        );
        assert_eq!(parse_lsappinfo_name("\"LSDisplayName\"=[ NULL ]"), None);
        assert_eq!(parse_lsappinfo_name(""), None);
    }
}
