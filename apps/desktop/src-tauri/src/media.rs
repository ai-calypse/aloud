//! Wiring the Media section (`aloud-media`) to the notch.
//!
//! Orchestration only: one poller that reads the players (and browser tabs)
//! and keeps the
//! closed-notch island honest, and a command per thing the views can do. What
//! a player reports, how lyrics are parsed and how devices are listed are the
//! crate's.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use aloud_islands::{Island, IslandAction, IslandKind, IslandPriority};
use aloud_media::devices::BtDevice;
use aloud_media::lyrics::Lyrics;
use aloud_media::sound::{Direction, SoundState};
use aloud_media::{NowPlaying, PlayState, Player, Reading, Transport};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::sync::Notify;

const LABEL: &str = "overlay";
const ISLAND: &str = "media";

/// The players, as of the last poll. Pushed whenever the track or state
/// changes, and on every poll while a Media view is on screen (the position
/// moves; the view interpolates between polls).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "media-now-playing")]
pub struct MediaNowPlaying(pub Reading);

/// How many Media views are on screen. Polling is 1 s while this is above
/// zero, and slower when it is not: the island only needs the track.
static WATCHERS: AtomicU32 = AtomicU32::new(0);
static LAST: Mutex<Option<Reading>> = Mutex::new(None);
static POLL_NOW: Notify = Notify::const_new();

/// Poll the moment a Media view opens or a transport button was pressed,
/// instead of up to a period later.
pub fn poll_now() {
    POLL_NOW.notify_one();
}

fn last() -> std::sync::MutexGuard<'static, Option<Reading>> {
    LAST.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether browser tabs are scanned: the Settings row, on by default. Read
/// every poll so flipping it applies at once.
fn browsers_on(app: &AppHandle) -> bool {
    app.try_state::<crate::AppState>()
        .and_then(|state| state.settings.lock().ok().map(|s| s.browser_media))
        .unwrap_or(true)
}

/// Whether the any-app helper may run: the Settings row, on by default.
fn any_app_on(app: &AppHandle) -> bool {
    app.try_state::<crate::AppState>()
        .and_then(|state| state.settings.lock().ok().map(|s| s.any_app_media))
        .unwrap_or(true)
}

/// Background polling (no Media view open) starts once the user has opened
/// Media once, so Spotify's Automation prompt appears when they ask for the
/// feature, not at launch.
fn enabled_marker() -> Option<std::path::PathBuf> {
    aloud_notes::data_dir()
        .ok()
        .map(|d| d.join("media-enabled"))
}

fn enabled() -> bool {
    enabled_marker().is_some_and(|p| p.exists())
}

fn enable() {
    if let Some(path) = enabled_marker().filter(|p| !p.exists()) {
        let _ = std::fs::write(path, "");
    }
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    // A change MediaRemote reports is shown at once, not at the next tick.
    aloud_media::mediaremote::start(poll_now);
    tauri::async_runtime::spawn(async move {
        let mut shown: Option<NowPlaying> = None;
        loop {
            let watched = WATCHERS.load(Ordering::Relaxed) > 0;
            let browsers = browsers_on(&app);
            // The helper runs only for a user who has asked for Media (the same
            // gate as the polling) and has not switched it off.
            aloud_media::mediaremote::set_enabled(any_app_on(&app) && (watched || enabled()));
            let reading = if watched || enabled() {
                tauri::async_runtime::spawn_blocking(move || {
                    aloud_media::now_playing::read(browsers)
                })
                .await
                .unwrap_or_default()
            } else {
                Reading::default()
            };
            let playing = reading
                .now
                .as_ref()
                .is_some_and(|n| n.state == PlayState::Playing);
            publish(&app, reading, watched, &mut shown);

            let period = match (watched, playing) {
                (true, _) => Duration::from_secs(1),
                (false, true) => Duration::from_secs(4),
                // Nothing audible: the island is gone, so only notice a start.
                (false, false) => Duration::from_secs(8),
            };
            tokio::select! {
                () = tokio::time::sleep(period) => {}
                () = POLL_NOW.notified() => {}
            }
        }
    });
}

/// Sends `reading` if it differs from the last in anything but the position
/// (or at all, while watched), and keeps the island in step with it.
fn publish(app: &AppHandle, reading: Reading, watched: bool, shown: &mut Option<NowPlaying>) {
    let changed = {
        let mut last = last();
        let same_shape = last.as_ref().is_some_and(|l| same_shape(l, &reading));
        let same = last.as_ref() == Some(&reading);
        *last = Some(reading.clone());
        !(same || (same_shape && !watched))
    };
    if changed {
        if let Err(e) = MediaNowPlaying(reading.clone()).emit_to(app, LABEL) {
            tracing::warn!(%e, "could not send now playing");
        }
    }
    sync_island(app, reading.now, shown);
}

/// Equal but for where in the track it is.
fn same_shape(a: &Reading, b: &Reading) -> bool {
    let strip = |r: &Reading| {
        let mut r = r.clone();
        if let Some(n) = r.now.as_mut() {
            n.position_ms = 0;
        }
        r
    };
    strip(a) == strip(b)
}

/// The island is the track while it is playing; paused or gone, it is not
/// there (the closed notch is not a place to park a song).
fn sync_island(app: &AppHandle, now: Option<NowPlaying>, shown: &mut Option<NowPlaying>) {
    let Some(now) = now.filter(|n| n.state == PlayState::Playing) else {
        if shown.take().is_some() {
            crate::islands::remove(app, ISLAND);
        }
        return;
    };
    let same_track = shown.as_ref().is_some_and(|s| s.key() == now.key());
    // A MediaRemote cover arrives after the title: same track, new cover.
    let same_cover = shown
        .as_ref()
        .is_some_and(|s| s.artwork_url == now.artwork_url);
    if same_track && same_cover {
        return;
    }
    if !same_track {
        crate::islands::push(app, island(&now, None));
    }
    *shown = Some(now.clone());
    // The cover can take a network round trip; the island is up without it
    // and takes it when it arrives.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // A page with no cover shows the browser's own icon: a web video's
        // island is otherwise a blank tile. Any other app shows its own.
        let art = match aloud_media::artwork(&now).await {
            Some(art) => Some(art),
            None if matches!(now.player, Player::Browser | Player::MediaRemote) => {
                let name = now.source.app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    aloud_media::now_playing::app_icon(&name)
                })
                .await
                .ok()
                .flatten()
            }
            None => None,
        };
        if let Some(art) = art {
            crate::islands::push(&app, island(&now, Some(art)));
        }
    });
}

/// What the island's button tells `actions.ts` to drive: the app's name for the
/// players and browsers (the way it always has), `mediaRemote` for the rest.
fn action_player(now: &NowPlaying) -> &str {
    match now.player {
        Player::MediaRemote => "mediaRemote",
        _ => &now.source.app,
    }
}

fn island(now: &NowPlaying, art: Option<String>) -> Island {
    Island {
        id: ISLAND.into(),
        kind: IslandKind::Live,
        priority: IslandPriority::Low,
        icon: art.or_else(|| {
            matches!(now.player, Player::Spotify | Player::Music).then(|| "music".into())
        }),
        // The row in Now › Overview reads artist over song; the pill, which
        // has one short value, shows the song.
        title: if now.artist.is_empty() {
            now.title.clone()
        } else {
            now.artist.clone()
        },
        detail: Some(now.title.clone()),
        actions: vec![IslandAction::new("Pause", "media.toggle").arg("player", action_player(now))],
        expires: None,
    }
}

/// What the players are doing right now, read fresh: a view that mounts
/// should not wait up to a second for its first frame.
#[tauri::command]
#[specta::specta]
pub async fn media_now_playing(app: AppHandle) -> Reading {
    let browsers = browsers_on(&app);
    let reading =
        tauri::async_runtime::spawn_blocking(move || aloud_media::now_playing::read(browsers))
            .await
            .unwrap_or_default();
    *last() = Some(reading.clone());
    reading
}

/// A Media view mounted (`true`) or went away (`false`).
#[tauri::command]
#[specta::specta]
pub fn media_watch(on: bool) {
    if on {
        enable();
        WATCHERS.fetch_add(1, Ordering::Relaxed);
        poll_now();
    } else {
        // Saturating: an unmount that outran its mount must not wrap to
        // "forever watched".
        let _ = WATCHERS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            Some(n.saturating_sub(1))
        });
    }
}

#[tauri::command]
#[specta::specta]
pub async fn media_control(player: Player, transport: Transport) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        aloud_media::now_playing::control(player, transport)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    settle_then_poll();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn media_seek(player: Player, position_ms: u32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        aloud_media::now_playing::seek(player, position_ms)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    settle_then_poll();
    Ok(())
}

/// Playback speed of the browser tab on screen, in percent (100 is normal).
#[tauri::command]
#[specta::specta]
pub async fn media_set_speed(player: Player, speed_pct: u16) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        aloud_media::now_playing::set_speed(player, speed_pct)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    settle_then_poll();
    Ok(())
}

/// An app's icon as a `data:` URI, for the badge saying where a track plays.
#[tauri::command]
#[specta::specta]
pub async fn media_source_icon(app: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || aloud_media::now_playing::app_icon(&app))
        .await
        .ok()
        .flatten()
}

/// "I turned it on": forget that a browser could not be read and look now,
/// instead of waiting out the few minutes between checks.
#[tauri::command]
#[specta::specta]
pub fn media_recheck_browsers() {
    aloud_media::browser::recheck();
    poll_now();
}

/// Brings a browser forward so its menu bar is within reach of the fix the
/// view describes.
#[tauri::command]
#[specta::specta]
pub fn media_open_browser(app: String) {
    aloud_media::browser::open(&app);
}

/// A player reports its old state for a moment after a command (the agent's
/// tools learned this the hard way), so the re-read waits a beat.
fn settle_then_poll() {
    tauri::async_runtime::spawn(async {
        tokio::time::sleep(Duration::from_millis(400)).await;
        poll_now();
    });
}

/// The cover of `now`, as a `data:` URI the overlay's CSP will load.
#[tauri::command]
#[specta::specta]
pub async fn media_artwork(now: NowPlaying) -> Option<String> {
    aloud_media::artwork(&now).await
}

/// Lyrics for `now` from LRCLIB, cached per track. `None` is "none exist";
/// an error is a network failure the view may retry.
#[tauri::command]
#[specta::specta]
pub async fn media_lyrics(now: NowPlaying) -> Result<Option<Lyrics>, String> {
    aloud_media::lyrics::lyrics(&now)
        .await
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
#[specta::specta]
pub async fn sound_state() -> SoundState {
    tauri::async_runtime::spawn_blocking(aloud_media::sound::state)
        .await
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub async fn sound_set_default(direction: Direction, id: u32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || aloud_media::sound::set_default(direction, id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
#[specta::specta]
pub async fn sound_set_mic_muted(muted: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || aloud_media::sound::set_mic_muted(muted))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

/// Paired Bluetooth devices and their batteries. Slow (the profiler takes a
/// second or two), so the view asks on a slow clock, not a fast one.
#[tauri::command]
#[specta::specta]
pub async fn bluetooth_devices() -> Result<Vec<BtDevice>, String> {
    tauri::async_runtime::spawn_blocking(aloud_media::devices::list)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
#[specta::specta]
pub async fn bluetooth_connect(address: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || aloud_media::devices::connect(&address))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

/// Opens System Settings at Automation, where Spotify and Music are allowed
/// or not: the one fix for a player that is running but says nothing.
#[tauri::command]
#[specta::specta]
pub fn open_automation_settings() {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation")
        .status();
}
