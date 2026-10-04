//! What Spotify or Music is playing, read and driven over the same
//! AppleScript the agent's `ALOUD_SPOTIFY` / `ALOUD_MUSIC` tools use.
//!
//! ponytail: only the two players with a scripting dictionary. "Any app"
//! (browser video, Podcasts) is MediaRemote, which is private and, from macOS
//! 15.4, readable only by Apple-signed processes; the upgrade is the
//! `mediaremote-adapter` helper run through `/usr/bin/perl`.

use aloud_native::{run, run_jxa, ScriptError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Player {
    Spotify,
    Music,
}

impl Player {
    /// The app's name, which is also its process name and its scripting target.
    pub fn app(self) -> &'static str {
        match self {
            Player::Spotify => "Spotify",
            Player::Music => "Music",
        }
    }
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
    pub state: PlayState,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// `u32`: specta forbids 64-bit ints on the wire, and 49 days of track fit.
    pub duration_ms: u32,
    pub position_ms: u32,
    /// Spotify's own https cover URL; Music has none and is read from the app.
    pub artwork_url: Option<String>,
}

impl NowPlaying {
    /// Names the track, not the moment: equal across polls of one track, so
    /// artwork and lyrics are fetched when it changes and never otherwise.
    pub fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.player.app(),
            self.title,
            self.artist,
            self.duration_ms / 1000
        )
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
/// playing" over a perfectly good song.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub now: Option<NowPlaying>,
    pub denied: Option<Player>,
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

/// Reads every running player and picks the one the user means: playing beats
/// paused, Spotify beats Music on a tie (Music ships with every Mac and is
/// often left open, so it is the likelier idle one).
pub fn read() -> Reading {
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
    readings.now = pick(found);
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
        Player::Music => v["duration"].as_f64().unwrap_or(0.0) * 1000.0,
    };
    Some(NowPlaying {
        player,
        state,
        title,
        artist: text("artist"),
        album: text("album"),
        duration_ms: duration_ms.max(0.0) as u32,
        position_ms: (v["position"].as_f64().unwrap_or(0.0).max(0.0) * 1000.0) as u32,
        artwork_url: v["art"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
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

pub fn control(player: Player, transport: Transport) -> Result<(), ScriptError> {
    let cmd = match transport {
        Transport::Play => "play",
        Transport::Pause => "pause",
        Transport::Toggle => "toggle",
        Transport::Next => "next",
        Transport::Previous => "previous",
    };
    run_jxa(
        player.app(),
        CONTROL,
        &json!({ "app": player.app(), "cmd": cmd }),
    )
    .map(drop)
}

pub fn seek(player: Player, position_ms: u32) -> Result<(), ScriptError> {
    let seconds = f64::from(position_ms) / 1000.0;
    run_jxa(
        player.app(),
        CONTROL,
        &json!({ "app": player.app(), "cmd": "seek", "seconds": seconds }),
    )
    .map(drop)
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
            state,
            title: "t".into(),
            artist: "a".into(),
            album: "b".into(),
            duration_ms: 1000,
            position_ms: 0,
            artwork_url: None,
        }
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
}
