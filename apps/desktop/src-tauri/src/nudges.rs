//! Nudges: other apps' notifications, an event about to start and a reminder
//! that has come due, as takeovers in the notch.
//!
//! Orchestration only. Reading Notification Center is `aloud_notify`; what an
//! island looks like is `aloud_notify::island` and `aloud_agenda::soon_island`;
//! whether it shows now is `islands::takeover`'s gate. This file starts and
//! stops the watcher with the setting, applies the allowlist, and keeps the two
//! calendar sources from announcing the same thing twice.
//!
//! Notification text passes through here and is never logged or stored.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use aloud_notify::{MirrorState, Notification, Watch};
use aloud_store::settings::NotchEvents;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

/// The watcher's state, sent when it changes: Settings says it in words.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "nudge-state")]
pub struct NudgeStateChanged {
    pub state: MirrorState,
}

static WATCH: Mutex<Option<Watch>> = Mutex::new(None);
static STATE: Mutex<MirrorState> = Mutex::new(MirrorState::Off);

fn lock<T>(m: &'static Mutex<T>) -> std::sync::MutexGuard<'static, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn set_state(app: &AppHandle, state: MirrorState) {
    *lock(&STATE) = state.clone();
    if let Err(e) = (NudgeStateChanged { state }).emit(app) {
        tracing::warn!(%e, "could not send the nudge state");
    }
}

/// Starts or stops the watcher to match the setting. Call at launch and on
/// every settings save; a save that changes nothing about it does nothing.
pub fn apply(app: &AppHandle, events: &NotchEvents) {
    let mut watch = lock(&WATCH);
    match (events.app_notifications, watch.is_some()) {
        (true, false) => *watch = Some(start(app)),
        (false, true) => {
            *watch = None;
            set_state(app, MirrorState::Off);
        }
        _ => {}
    }
}

fn start(app: &AppHandle) -> Watch {
    let (for_state, for_new) = (app.clone(), app.clone());
    aloud_notify::watch(
        move |state| set_state(&for_state, state.clone()),
        move |n| on_notification(&for_new, n),
    )
}

/// What the Full Disk Access row shows.
#[tauri::command]
#[specta::specta]
pub fn nudge_state() -> MirrorState {
    lock(&STATE).clone()
}

/// Try again, after the user granted access: the watcher stops on a refusal
/// rather than polling for the grant, so this is the way back.
#[tauri::command]
#[specta::specta]
pub fn nudge_recheck(app: AppHandle) {
    let enabled = app
        .try_state::<crate::AppState>()
        .and_then(|s| {
            s.settings
                .lock()
                .ok()
                .map(|s| s.notch_events.app_notifications)
        })
        .unwrap_or(false);
    let mut watch = lock(&WATCH);
    *watch = enabled.then(|| start(&app));
}

/// System Settings at Full Disk Access, the one fix for `NeedsAccess`.
#[tauri::command]
#[specta::specta]
pub fn nudge_open_access_settings() {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        .status();
}

/// The Open button: brings the app that sent a nudge forward. The webview
/// names the bundle, so it is checked to be a bundle id and nothing else.
#[tauri::command]
#[specta::specta]
pub fn nudge_open(bundle: String) -> Result<(), String> {
    let valid = !bundle.is_empty()
        && bundle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if !valid {
        return Err("that is not an app".into());
    }
    #[cfg(target_os = "macos")]
    let opened = std::process::Command::new("open")
        .args(["-b", &bundle])
        .status()
        .is_ok_and(|s| s.success());
    #[cfg(not(target_os = "macos"))]
    let opened = false;
    if opened {
        Ok(())
    } else {
        Err("could not open it".into())
    }
}

// ---- mirrored notifications -------------------------------------------------

/// How long a call card stays if nobody dismisses it: a ring is over by then,
/// and Notification Center's own record of it is not read back to find out.
const RING: Duration = Duration::from_secs(45);

/// A mirrored Calendar or Reminders banner and our own agenda peek are two
/// voices for one thing; whichever came second within this stays quiet.
const SAME_THING: Duration = Duration::from_secs(180);

static MIRRORED_AGENDA_AT: Mutex<Option<Instant>> = Mutex::new(None);
static NATIVE_AGENDA_AT: Mutex<Option<Instant>> = Mutex::new(None);

fn within(slot: &'static Mutex<Option<Instant>>) -> bool {
    lock(slot).is_some_and(|at| at.elapsed() < SAME_THING)
}

fn is_agenda_app(bundle: &str) -> bool {
    matches!(bundle, "com.apple.iCal" | "com.apple.reminders")
}

fn on_notification(app: &AppHandle, n: Notification) {
    let events = app
        .try_state::<crate::AppState>()
        .and_then(|s| s.settings.lock().ok().map(|s| s.notch_events.clone()))
        .unwrap_or_default();
    // Re-checked per notification, not only at start: the allowlist applies
    // the moment it is edited, and a row that raced a switch-off is dropped.
    if !events.app_notifications || !events.app_notification_allow.contains(&n.app) {
        return;
    }
    if is_agenda_app(&n.app) {
        if within(&NATIVE_AGENDA_AT) {
            return;
        }
        *lock(&MIRRORED_AGENDA_AT) = Some(Instant::now());
    }
    let name = app_name(&n.app);
    let island = aloud_notify::island(&n, &name, aloud_selection::app_icon(&name));
    // The app and a count: never what it said.
    tracing::debug!(app = %n.app, "nudge");
    let id = island.id.clone();
    let ringing = island.tier == Some(aloud_islands::IslandTier::Card);
    crate::islands::takeover(app, island);
    if ringing {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(RING).await;
            crate::islands::remove(&app, &id);
        });
    }
}

/// The name macOS shows for a bundle: the ones every Mac shares, else the
/// installed app, else the last part of the id.
fn app_name(bundle: &str) -> String {
    if let Some(name) = aloud_notify::known_name(bundle) {
        return name.into();
    }
    aloud_selection::installed_apps()
        .into_iter()
        .find(|a| a.bundle_id.as_deref() == Some(bundle))
        .map(|a| a.name)
        .unwrap_or_else(|| bundle.rsplit('.').next().unwrap_or(bundle).to_string())
}

// ---- calendar soon, reminder due -------------------------------------------

/// Ids already announced, with when: an event is "soon" for ten minutes of one-
/// minute ticks and must speak once. Pruned by age so a Mac that stays up for
/// weeks does not keep every id it ever saw.
static SEEN: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

fn first_time(id: &str) -> bool {
    let mut seen = lock(&SEEN);
    let seen = seen.get_or_insert_with(HashMap::new);
    seen.retain(|_, at| at.elapsed() < Duration::from_secs(24 * 3600));
    seen.insert(id.to_string(), Instant::now()).is_none()
}

/// Set when Today › Reminders read the list once: only then does the tick read
/// it too. A background JXA call to Reminders is what makes macOS ask for
/// Automation access, and that question should come from a view the user
/// opened, not out of nowhere.
pub static REMINDERS_ALLOWED: AtomicBool = AtomicBool::new(false);

fn raise_agenda(app: &AppHandle, mut island: aloud_islands::Island, name: &str) {
    if !first_time(&island.id) {
        return;
    }
    if within(&MIRRORED_AGENDA_AT) {
        return;
    }
    *lock(&NATIVE_AGENDA_AT) = Some(Instant::now());
    // The app's own icon, which a glyph name cannot be.
    island.icon = aloud_selection::app_icon(name).or(island.icon);
    crate::islands::takeover(app, island);
}

/// Called each minute with the cached events: raises a High peek for any that
/// start within ten minutes.
pub fn calendar_tick(app: &AppHandle, events: &[aloud_agenda::Event], now: i64) {
    for event in aloud_agenda::soon(events, now) {
        raise_agenda(app, aloud_agenda::soon_island(event, now), "Calendar");
    }
}

/// Called every other minute: reads Reminders if it can do so without launching it (EventKit, or the app already open) and was allowed,
/// then raises a Normal peek for each that just came due.
pub async fn reminders_tick(app: &AppHandle, now: i64) {
    let readable = aloud_agenda::reminders_in_process() || aloud_agenda::app_running("Reminders");
    if !REMINDERS_ALLOWED.load(Ordering::Relaxed) || !readable {
        return;
    }
    let read =
        tauri::async_runtime::spawn_blocking(move || aloud_agenda::reminders(now, false)).await;
    let Ok(Ok(reminders)) = read else { return };
    for reminder in aloud_agenda::due(&reminders, now) {
        raise_agenda(app, aloud_agenda::due_island(reminder), "Reminders");
    }
}
