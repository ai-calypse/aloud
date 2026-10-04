//! What Spotify, Music or a browser tab is playing, read and driven over the
//! same AppleScript the agent's `ALOUD_SPOTIFY` / `ALOUD_MUSIC` tools use.
//!
//! ponytail: the two players with a scripting dictionary, plus browser tabs
//! through [`crate::browser`]. "Any app" (Podcasts, VLC) is MediaRemote, which
//! is private and, from macOS 15.4, readable only by Apple-signed processes;
//! the upgrade is the `mediaremote-adapter` helper run through
//! `/usr/bin/perl`.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use aloud_native::{run, run_jxa, ScriptError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::browser::{self, Cmd, Issue};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Player {
    Spotify,
    Music,
    /// A tab in a browser; which one is [`NowPlaying::source`].
    Browser,
}

impl Player {
    /// The app's name, which is also its process name and its scripting target.
    /// A browser is several apps: ask [`NowPlaying::source`].
    pub fn app(self) -> &'static str {
        match self {
            Player::Spotify => "Spotify",
            Player::Music => "Music",
            Player::Browser => "Browser",
        }
    }
}

/// Where a track is playing: the app, and for a browser the page's host and
/// which tab (an opaque "window/tab" the controls hand back to the browser).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub app: String,
    pub host: Option<String>,
    pub tab: Option<String>,
}

impl Source {
    fn app(app: &str) -> Self {
        Source { app: app.into(), host: None, tab: None }
    }

    /// One per player, one per tab: what "most recently playing" is tracked by.
    fn id(&self) -> String {
        format!("{}/{}", self.app, self.tab.as_deref().unwrap_or_default())
    }
}

/// A track and artist the page names cleanly enough to ask for lyrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    pub title: String,
    pub artist: String,
}

/// Stopped is not here: a player with nothing loaded is "nothing playing".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PlayState {
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub player: Player,
    pub source: Source,
    pub state: PlayState,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// `u32`: specta forbids 64-bit ints on the wire, and 49 days of track fit.
    pub duration_ms: u32,
    pub position_ms: u32,
    /// Playback speed in percent (100 is normal). Only a web page has one; the
    /// players report 100.
    pub speed_pct: u16,
    /// Spotify's own https cover URL, a YouTube thumbnail, or a page's
    /// `og:image`; Music has none and is read from the app.
    pub artwork_url: Option<String>,
    /// Browser only, and only when the title and channel parse cleanly.
    pub song: Option<Song>,
}

impl NowPlaying {
    /// Names the track, not the moment: equal across polls of one track, so
    /// artwork and lyrics are fetched when it changes and never otherwise.
    pub fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.source.app,
            self.title,
            self.artist,
            self.duration_ms / 1000
        )
    }

    /// What to look lyrics up by: the players' own metadata, but for a web
    /// video only a title and artist the page named cleanly.
    pub fn lyrics_query(&self) -> Option<(&str, &str)> {
        match self.player {
            Player::Browser => self.song.as_ref().map(|s| (s.title.as_str(), s.artist.as_str())),
            _ => Some((self.title.as_str(), self.artist.as_str())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Transport {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
}

/// One look at the players. `denied` is a player that is running but has not
/// granted the Automation permission: without it the view would say "nothing
/// playing" over a perfectly good song. `browsers` is the same for running
/// browsers, which have a second switch (JavaScript from Apple Events).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub now: Option<NowPlaying>,
    pub denied: Option<Player>,
    pub browsers: Vec<Issue>,
}

const READ: &str = r#"
const A = Application(ARG.app);
const state = A.playerState();
if (state === "stopped") return JSON.stringify({ state });
const g = (f) => { try { return f(); } catch (e) { return null; } };
const t = A.currentTrack;
return JSON.stringify({
  state, title: g(() => t.name()), artist: g(() => t.artist()), album: g(() => t.album()),
  duration: g(() => t.duration()), position: g(() => A.playerPosition()),
  art: ARG.app === "Spotify" ? g(() => t.artworkUrl()) : null,
});"#;

const CONTROL: &str = r#"
const A = Application(ARG.app);
if (ARG.cmd === "play") A.play();
else if (ARG.cmd === "pause") A.pause();
else if (ARG.cmd === "toggle") A.playpause();
else if (ARG.cmd === "next") A.nextTrack();
else if (ARG.cmd === "previous") A.previousTrack();
else if (ARG.cmd === "seek") A.playerPosition = ARG.seconds;
return "";"#;

/// Is the app's process alive? `Application(..).running()` would do, but a
/// second of osascript to ask is a lot to spend every four seconds, and
/// scripting an app that is not running *launches* it.
#[cfg(target_os = "macos")]
pub fn running(player: Player) -> bool {
    std::process::Command::new("pgrep")
        .args(["-xq", player.app()])
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(not(target_os = "macos"))]
pub fn running(_: Player) -> bool {
    false
}

/// Which tab the last reading chose, for the controls to act on.
static PICKED: Mutex<Option<Source>> = Mutex::new(None);
static RECENCY: Mutex<Option<Recency>> = Mutex::new(None);

/// When each source started playing, and when it last was, so that of two
/// things playing the newer wins: the video just opened over the song left
/// running, and the other way round once the song is restarted.
#[derive(Default)]
struct Recency {
    tick: u64,
    since: HashMap<String, u64>,
    last: HashMap<String, u64>,
}

impl Recency {
    /// Sorts `found` most-recent first. Stable, so sources that tie (all of
    /// them, on the first read) keep the order they were read in.
    fn order(&mut self, found: &mut [NowPlaying]) {
        self.tick += 1;
        let playing = |n: &NowPlaying| n.state == PlayState::Playing;
        let live: HashSet<String> = found.iter().filter(|n| playing(n)).map(|n| n.source.id()).collect();
        let all: HashSet<String> = found.iter().map(|n| n.source.id()).collect();
        self.since.retain(|k, _| live.contains(k));
        // A closed tab stops reporting; its history goes with it.
        self.last.retain(|k, _| all.contains(k));
        for k in live {
            self.since.entry(k.clone()).or_insert(self.tick);
            self.last.insert(k, self.tick);
        }
        let rank = |n: &NowPlaying| {
            let id = n.source.id();
            let table = if playing(n) { &self.since } else { &self.last };
            table.get(&id).copied().unwrap_or(0)
        };
        found.sort_by_key(|n| std::cmp::Reverse(rank(n)));
    }
}

/// Reads every running player (and, with `browsers`, every browser tab) and
/// picks the one the user means: playing beats paused, the one that started
/// most recently beats an older one, and Spotify beats Music on a tie (Music
/// ships with every Mac and is often left open, so it is the likelier idle
/// one).
pub fn read(browsers: bool) -> Reading {
    let mut readings = Reading::default();
    let mut found = Vec::new();
    for player in [Player::Spotify, Player::Music] {
        if !running(player) {
            continue;
        }
        match run_jxa(player.app(), READ, &json!({ "app": player.app() })) {
            Ok(out) => found.extend(parse(player, &out)),
            Err(ScriptError::NotPermitted { .. }) => readings.denied = Some(player),
            Err(e) => tracing::debug!(%e, app = player.app(), "now-playing read failed"),
        }
    }
    if browsers {
        let (tabs, issues) = browser::scan();
        found.extend(tabs);
        readings.browsers = issues;
    }
    RECENCY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_default()
        .order(&mut found);
    readings.now = pick(found);
    *PICKED.lock().unwrap_or_else(PoisonError::into_inner) =
        readings.now.as_ref().map(|n| n.source.clone());
    readings
}

/// The player's JSON as a track, or None when it is stopped or not yet
/// reporting a title (a track loading reports nulls for a moment).
fn parse(player: Player, out: &str) -> Option<NowPlaying> {
    let v: Value = serde_json::from_str(out).ok()?;
    let state = match v["state"].as_str()? {
        "playing" => PlayState::Playing,
        "paused" => PlayState::Paused,
        _ => return None,
    };
    let text = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    let title = text("title");
    if title.is_empty() {
        return None;
    }
    // Spotify reports milliseconds and Music seconds, both for the duration
    // and (in seconds for both) the position.
    let duration_ms = match player {
        Player::Spotify => v["duration"].as_f64().unwrap_or(0.0),
        Player::Music | Player::Browser => v["duration"].as_f64().unwrap_or(0.0) * 1000.0,
    };
    Some(NowPlaying {
        player,
        source: Source::app(player.app()),
        state,
        title,
        artist: text("artist"),
        album: text("album"),
        duration_ms: duration_ms.max(0.0) as u32,
        position_ms: (v["position"].as_f64().unwrap_or(0.0).max(0.0) * 1000.0) as u32,
        speed_pct: 100,
        artwork_url: v["art"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        song: None,
    })
}

/// `found` is in tie-break order (Spotify first), and `max_by_key` keeps the
/// *last* maximum, so the order is reversed first.
fn pick(found: Vec<NowPlaying>) -> Option<NowPlaying> {
    found
        .into_iter()
        .rev()
        .max_by_key(|n| n.state == PlayState::Playing)
}

/// The tab the last reading chose, which is what a browser button means.
fn picked_tab() -> Result<Source, ScriptError> {
    PICKED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .filter(|s| s.tab.is_some())
        .ok_or_else(|| ScriptError::Failed("no browser tab is playing".into()))
}

pub fn control(player: Player, transport: Transport) -> Result<(), ScriptError> {
    let cmd = match transport {
        Transport::Play => "play",
        Transport::Pause => "pause",
        Transport::Toggle => "toggle",
        Transport::Next => "next",
        Transport::Previous => "previous",
    };
    if player == Player::Browser {
        let cmd = match transport {
            Transport::Play => Cmd::Play,
            Transport::Pause => Cmd::Pause,
            Transport::Toggle => Cmd::Toggle,
            Transport::Next => Cmd::Next,
            Transport::Previous => Cmd::Previous,
        };
        return browser::control(&picked_tab()?, cmd);
    }
    run_jxa(
        player.app(),
        CONTROL,
        &json!({ "app": player.app(), "cmd": cmd }),
    )
    .map(drop)
}

pub fn seek(player: Player, position_ms: u32) -> Result<(), ScriptError> {
    let seconds = f64::from(position_ms) / 1000.0;
    if player == Player::Browser {
        return browser::control(&picked_tab()?, Cmd::SeekSeconds(seconds));
    }
    run_jxa(
        player.app(),
        CONTROL,
        &json!({ "app": player.app(), "cmd": "seek", "seconds": seconds }),
    )
    .map(drop)
}

/// Playback speed for the browser tab; Spotify and Music have none.
pub fn set_speed(player: Player, speed_pct: u16) -> Result<(), ScriptError> {
    if player != Player::Browser {
        return Err(ScriptError::Failed(format!("{} has no playback speed", player.app())));
    }
    browser::control(&picked_tab()?, Cmd::Speed(f64::from(speed_pct) / 100.0))
}

/// An app's icon as a `data:` URI, for the badge that says where a track is
/// playing. Cached per app by `aloud_selection`.
pub fn app_icon(app: &str) -> Option<String> {
    aloud_selection::app_icon(app)
}

/// The current Music track's cover, as the file bytes. AppleScript, not JXA:
/// writing `raw data` to a file is the one route to the picture that is
/// documented to work, and JXA's bridge for the type is not.
pub fn music_artwork() -> Option<Vec<u8>> {
    let path = std::env::temp_dir().join(format!("aloud-art-{}", std::process::id()));
    let script = format!(
        r#"tell application "Music"
            if (count of artworks of current track) is 0 then return "none"
            set d to raw data of artwork 1 of current track
        end tell
        set f to open for access (POSIX file "{}") with write permission
        set eof f to 0
        write d to f
        close access f
        return "ok""#,
        path.display()
    );
    let out = run("Music", &script).ok()?;
    let bytes = (out == "ok").then(|| std::fs::read(&path).ok()).flatten();
    let _ = std::fs::remove_file(&path);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(player: Player, state: PlayState) -> NowPlaying {
        NowPlaying {
            player,
            source: Source::app(player.app()),
            state,
            title: "t".into(),
            artist: "a".into(),
            album: "b".into(),
            duration_ms: 1000,
            position_ms: 0,
            speed_pct: 100,
            artwork_url: None,
            song: None,
        }
    }

    fn tab(id: &str, state: PlayState) -> NowPlaying {
        let mut n = track(Player::Browser, state);
        n.source = Source { app: "Brave Browser".into(), host: Some("youtube.com".into()), tab: Some(id.into()) };
        n
    }

    /// Reads as `Recency` + `pick` do in `read`, one poll per call.
    fn poll(r: &mut Recency, mut found: Vec<NowPlaying>) -> NowPlaying {
        r.order(&mut found);
        pick(found).unwrap()
    }

    /// Guards: Spotify answers its duration in milliseconds and Music in
    /// seconds, and reading both the same way made a 3-minute Music track 3 s.
    #[test]
    fn duration_units_differ_per_player() {
        let spotify = r#"{"state":"playing","title":"S","artist":"A","album":"B","duration":215000,"position":12.5,"art":"https://i.scdn.co/x"}"#;
        let music = r#"{"state":"paused","title":"S","artist":"A","album":"B","duration":215.4,"position":12.5,"art":null}"#;

        let s = parse(Player::Spotify, spotify).unwrap();
        let m = parse(Player::Music, music).unwrap();

        assert_eq!((s.duration_ms, s.position_ms), (215_000, 12_500));
        assert_eq!((m.duration_ms, m.position_ms), (215_400, 12_500));
        assert_eq!(s.artwork_url.as_deref(), Some("https://i.scdn.co/x"));
        assert_eq!(m.state, PlayState::Paused);
    }

    #[test]
    fn stopped_and_untitled_tracks_are_nothing_playing() {
        assert_eq!(parse(Player::Music, r#"{"state":"stopped"}"#), None);
        let loading =
            r#"{"state":"playing","title":null,"artist":null,"duration":null,"position":null}"#;
        assert_eq!(parse(Player::Spotify, loading), None);
        assert_eq!(parse(Player::Spotify, "not json"), None);
    }

    /// Guards: an idle Music left open hiding the song Spotify is playing.
    #[test]
    fn playing_beats_paused_and_spotify_wins_ties() {
        let paused_spotify = track(Player::Spotify, PlayState::Paused);
        let playing_music = track(Player::Music, PlayState::Playing);
        let picked = pick(vec![paused_spotify.clone(), playing_music.clone()]).unwrap();
        assert_eq!(picked.player, Player::Music);

        let picked = pick(vec![
            paused_spotify,
            track(Player::Music, PlayState::Paused),
        ])
        .unwrap();
        assert_eq!(picked.player, Player::Spotify);
        assert_eq!(pick(vec![]), None);
    }

    #[test]
    fn the_key_ignores_position_and_state() {
        let mut a = track(Player::Spotify, PlayState::Playing);
        let mut b = a.clone();
        b.position_ms = 99_000;
        b.state = PlayState::Paused;
        assert_eq!(a.key(), b.key());
        a.title = "other".into();
        assert_ne!(a.key(), b.key());
    }

    /// Guards: a song left playing hiding the video the user just opened, and
    /// the reverse once they go back to the song. Of two playing, the one that
    /// started most recently is shown.
    #[test]
    fn the_most_recently_started_playing_source_wins() {
        let mut r = Recency::default();
        let spotify = || track(Player::Spotify, PlayState::Playing);
        // Both start together: Spotify, by order.
        assert_eq!(poll(&mut r, vec![spotify(), tab("1/1", PlayState::Playing)]).player, Player::Spotify);
        // Alone, then a tab starts: the tab.
        let mut r = Recency::default();
        assert_eq!(poll(&mut r, vec![spotify()]).player, Player::Spotify);
        assert_eq!(poll(&mut r, vec![spotify(), tab("1/1", PlayState::Playing)]).player, Player::Browser);
        // Stays the tab on the next poll, not flipping back by order.
        assert_eq!(poll(&mut r, vec![spotify(), tab("1/1", PlayState::Playing)]).player, Player::Browser);
        // The tab pauses: playing Spotify beats a paused tab.
        assert_eq!(poll(&mut r, vec![spotify(), tab("1/1", PlayState::Paused)]).player, Player::Spotify);
        // Spotify pauses and the tab resumes: it starts again, so it is newest.
        assert_eq!(poll(&mut r, vec![track(Player::Spotify, PlayState::Paused), tab("1/1", PlayState::Playing)]).player, Player::Browser);
    }

    /// Guards: two paused sources choosing by position in the list rather than
    /// by which was playing last.
    #[test]
    fn of_two_paused_sources_the_one_playing_last_is_shown() {
        let mut r = Recency::default();
        poll(&mut r, vec![tab("1/1", PlayState::Playing), tab("1/2", PlayState::Paused)]);
        let both_paused = || vec![tab("1/2", PlayState::Paused), tab("1/1", PlayState::Paused)];
        assert_eq!(poll(&mut r, both_paused()).source.tab.as_deref(), Some("1/1"));
        // A closed tab's history does not pile up.
        poll(&mut r, vec![tab("1/2", PlayState::Paused)]);
        assert!(r.last.is_empty());
    }

    #[test]
    fn web_video_lyrics_need_a_clean_song() {
        let mut n = tab("1/1", PlayState::Playing);
        assert_eq!(n.lyrics_query(), None);
        n.song = Some(Song { title: "Hello".into(), artist: "Adele".into() });
        assert_eq!(n.lyrics_query(), Some(("Hello", "Adele")));
        assert_eq!(track(Player::Spotify, PlayState::Playing).lyrics_query(), Some(("t", "a")));
    }
}
