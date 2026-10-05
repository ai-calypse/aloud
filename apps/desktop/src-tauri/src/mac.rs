//! Wiring `aloud_mac` to commands and the island queue.
//!
//! Orchestration only: what is read, parsed, thresholded and snapped is the
//! crate's. Views poll the read commands while they are on screen; the two
//! watchers below run for the app's life, because an island has to appear
//! while the notch is closed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aloud_islands::IslandKind;
use aloud_mac::events::{self, Event, Sources};
use aloud_mac::power::{self, Battery};
use aloud_mac::privacy::{self, PrivacyUse};
use aloud_mac::stats::{Sampler, SystemStats};
use aloud_mac::windows::{self, Snap};
use aloud_mac::{Step, Watcher, Watchers};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

const PRIVACY_EVERY: Duration = Duration::from_secs(2);
/// The OS says when a power source changed (`events::Event::BatteryMoved`), so
/// `pmset` is read then and not on a timer. Set once at launch for the first
/// reading, and again when the switch is turned back on.
static BATTERY_DIRTY: AtomicBool = AtomicBool::new(true);

static WATCHERS: Mutex<Watchers> = Mutex::new(Watchers {
    privacy: true,
    battery: true,
});
/// Shared, not the thread's own: `dismiss_island` tells it the user closed the
/// critical island, and the next poll must not put it back.
static BATTERY: Mutex<power::Watch> = Mutex::new(power::Watch::new());
static SAMPLER: Mutex<Option<Sampler>> = Mutex::new(None);

fn watchers() -> Watchers {
    *WATCHERS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Starts the watchers. Call once from `setup`.
pub fn start(app: &AppHandle) {
    if let Ok(path) = Watchers::path() {
        *WATCHERS.lock().unwrap_or_else(|e| e.into_inner()) = Watchers::load_from(&path);
    }
    start_events(app);
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("mac-watch".into())
        .spawn(move || {
            let mut privacy = privacy::Watch::default();
            loop {
                let on = watchers();
                if on.privacy {
                    let read = privacy::read();
                    if let Some(step) = privacy.update(&read) {
                        apply(&app, step);
                    }
                    if let Some(seen) = privacy.take_sighting(&read) {
                        crate::islands::takeover(&app, seen);
                    }
                }
                if on.battery && BATTERY_DIRTY.swap(false, Ordering::Relaxed) {
                    let read = power::read();
                    let steps = BATTERY
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .update(read.as_ref());
                    for step in steps {
                        apply(&app, step);
                    }
                }
                std::thread::sleep(PRIVACY_EVERY);
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(%e, "could not start the mac watchers");
    }
}

/// Charger, Low Power Mode, external displays, Wi-Fi and VPN, pushed by the
/// OS. The sources go on the main run loop, which only the main thread can
/// install them on (`aloud_mac::events` says why).
fn start_events(app: &AppHandle) {
    let handle = app.clone();
    let emit: events::Emit = Arc::new(move |event| match event.island() {
        // A whisper: the gate weighs it against Focus, typing and the user's
        // Power / Connectivity switches.
        Some(island) => crate::islands::takeover(&handle, island),
        None => {
            debug_assert_eq!(event, Event::BatteryMoved);
            BATTERY_DIRTY.store(true, Ordering::Relaxed);
        }
    });
    if let Err(e) = app.run_on_main_thread(move || events::start(Sources::ALL, emit)) {
        tracing::warn!(%e, "could not start the system event sources");
    }
}

/// The user closed an island. Only the battery's remembers it.
pub fn island_dismissed(id: &str) {
    if id == power::LOW_ID {
        BATTERY.lock().unwrap_or_else(|e| e.into_inner()).dismiss();
    }
}

fn apply(app: &AppHandle, step: Step) {
    match step {
        Step::Remove(id) => crate::islands::remove(app, id),
        // An alert is a takeover: the gate decides whether it shows, waits or
        // is not worth the notch's attention (a panel open, Focus, typing).
        Step::Push(island) if island.kind == IslandKind::Alert => {
            crate::islands::takeover(app, island);
        }
        Step::Push(island) => crate::islands::push(app, island),
    }
}

/// CPU, memory, disk, network and thermal pressure. Polled by Mac › Stats
/// only while it is on screen.
#[tauri::command]
#[specta::specta]
pub async fn mac_stats() -> Result<SystemStats, String> {
    // Blocks a quarter second between CPU reads when the last was long ago.
    tauri::async_runtime::spawn_blocking(|| {
        let mut sampler = SAMPLER.lock().unwrap_or_else(|e| e.into_inner());
        sampler.get_or_insert_with(Sampler::new).sample()
    })
    .await
    .map_err(|e| e.to_string())
}

/// The battery, or None on a Mac without one.
#[tauri::command]
#[specta::specta]
pub async fn mac_battery() -> Option<Battery> {
    tauri::async_runtime::spawn_blocking(power::read)
        .await
        .ok()
        .flatten()
}

/// Who holds the microphone and camera right now.
#[tauri::command]
#[specta::specta]
pub async fn mac_privacy() -> PrivacyUse {
    tauri::async_runtime::spawn_blocking(privacy::read)
        .await
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FrontWindowInfo {
    /// Accessibility granted: without it a snap cannot move anything.
    pub trusted: bool,
    /// The app whose window a snap would move.
    pub app: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn mac_front_window() -> FrontWindowInfo {
    tauri::async_runtime::spawn_blocking(|| {
        let trusted = windows::is_trusted();
        FrontWindowInfo {
            trusted,
            app: trusted.then(windows::front_window).flatten().map(|w| w.app),
        }
    })
    .await
    .unwrap_or(FrontWindowInfo {
        trusted: false,
        app: None,
    })
}

/// Snaps the window the user was in. Returns the app's name.
#[tauri::command]
#[specta::specta]
pub async fn mac_snap(app: AppHandle, snap: Snap) -> Result<String, String> {
    // NSScreen is main-thread only; the Accessibility calls are not, and may
    // block on a hung app, so only the displays are read there.
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(windows::visible_screens());
    })
    .map_err(|e| e.to_string())?;
    let screens = rx.await.map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || windows::snap(snap, &screens))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
#[specta::specta]
pub fn mac_watchers() -> Watchers {
    watchers()
}

/// A watcher's on/off. Turning one off takes its island down at once rather
/// than leaving it up until the next poll would have.
#[tauri::command]
#[specta::specta]
pub fn set_mac_watcher(app: AppHandle, watcher: Watcher, on: bool) -> Result<(), String> {
    let snapshot = {
        let mut current = WATCHERS.lock().unwrap_or_else(|e| e.into_inner());
        current.set(watcher, on);
        *current
    };
    if on && watcher == Watcher::Battery {
        BATTERY_DIRTY.store(true, Ordering::Relaxed);
    }
    if !on {
        match watcher {
            Watcher::Privacy => {
                crate::islands::remove(&app, privacy::ID);
                crate::islands::remove(&app, privacy::SEEN_ID);
            }
            Watcher::Battery => {
                crate::islands::remove(&app, power::LOW_ID);
                crate::islands::remove(&app, power::LOW_CHIP_ID);
                crate::islands::remove(&app, power::FULL_ID);
            }
        }
    }
    Watchers::path()
        .and_then(|path| snapshot.save_to(&path))
        .map_err(|e| format!("{e:#}"))
}
