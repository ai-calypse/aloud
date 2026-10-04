//! Wiring for the Focus section: one timer, one keep-awake, the islands they
//! show, and the second-by-second task that asks the timer what time it is.
//!
//! Orchestration only. The timer's arithmetic, the island wording and the
//! streak live in `aloud_focus` and `aloud_store::focus`, under test there.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use aloud_focus::{
    island, Awake, AwakeState, Session, SessionKind, Timer, TimerMode, TimerPreset, TimerState,
};
use aloud_store::{FocusLog, FocusStats};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;
use tokio::sync::Notify;

/// The timer and keep-awake as the webview draws them. Pushed whenever either
/// changes by anything other than the clock; the webview counts the seconds
/// itself against `TimerState`'s wall-clock fields.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "focus")]
pub struct FocusSnapshot {
    pub timer: TimerState,
    pub awake: AwakeState,
}

const LABEL: &str = "overlay";

/// How often keep-awake is checked while no timer is running: only to notice a
/// `caffeinate` that ended without us, and to refresh the minutes left.
const AWAKE_POLL: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct Focus {
    timer: Mutex<Timer>,
    awake: Mutex<Awake>,
    log: Mutex<Option<FocusLog>>,
    /// Wakes the ticker when something starts, so it sleeps while idle.
    wake: Notify,
    /// What the webview last heard about keep-awake, to emit when it changes
    /// without being asked (a timeout, a kill).
    last_awake: Mutex<AwakeState>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic elsewhere must not take the timer down with it.
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn init(app: &AppHandle) {
    let focus = Focus::default();
    let opened = aloud_notes::data_dir().and_then(|dir| FocusLog::open(&dir.join("focus.sqlite")));
    match opened {
        Ok(log) => *lock(&focus.log) = Some(log),
        Err(e) => tracing::warn!(%e, "focus streaks disabled"),
    }
    app.manage(focus);
    let app = app.clone();
    tauri::async_runtime::spawn(ticker(app));
}

fn snapshot(focus: &Focus) -> FocusSnapshot {
    let timer = lock(&focus.timer).state();
    let awake = lock(&focus.awake).state();
    FocusSnapshot { timer, awake }
}

/// Shows the state everywhere it is shown: the islands, and the webview.
fn publish(app: &AppHandle, focus: &Focus, emit: bool) {
    let snap = snapshot(focus);
    let now = now_ms();
    sync_island(app, island::TIMER_ID, island::timer(&snap.timer, now));
    sync_island(app, island::AWAKE_ID, island::awake(&snap.awake, now));
    *lock(&focus.last_awake) = snap.awake;
    if emit {
        if let Err(e) = snap.emit_to(app, LABEL) {
            tracing::warn!(%e, "could not send focus state");
        }
    }
    focus.wake.notify_one();
}

fn sync_island(app: &AppHandle, id: &str, island: Option<aloud_islands::Island>) {
    match island {
        Some(island) => crate::islands::push(app, island),
        None => crate::islands::remove(app, id),
    }
}

fn record(focus: &Focus, session: Session) {
    let kind = match session.kind {
        SessionKind::Pomodoro => "pomodoro",
        SessionKind::Stopwatch => "stopwatch",
    };
    let day = chrono::Local::now().date_naive();
    if let Some(log) = lock(&focus.log).as_ref() {
        if let Err(e) = log.record(day, kind, session.secs) {
            tracing::warn!(%e, "could not record a focus session");
        }
    }
}

/// Asks the timer the time once a second while one runs, and keep-awake
/// every few seconds while it holds. Nothing counts here: a tick that arrives
/// late — the lid was shut — just asks later and gets the right answer.
async fn ticker(app: AppHandle) {
    loop {
        let focus = app.state::<Focus>();
        let timer_running = lock(&focus.timer).is_running();
        let awake_on = lock(&focus.awake).state().active;
        if !timer_running && !awake_on {
            focus.wake.notified().await;
            continue;
        }
        // To the next whole second of the wall clock, so the pill's digits
        // change when the face does rather than drifting half a second off.
        let wait = if timer_running {
            Duration::from_millis(1000 - now_ms().rem_euclid(1000) as u64)
        } else {
            AWAKE_POLL
        };
        let _ = tokio::time::timeout(wait, focus.wake.notified()).await;

        let finished = lock(&focus.timer).tick(now_ms());
        if let Some(done) = finished {
            if let Some(session) = done.session {
                record(&focus, session);
            }
            crate::islands::push(&app, island::finished(&done));
        }
        let awake_changed = *lock(&focus.last_awake) != lock(&focus.awake).state();
        publish(&app, &focus, finished.is_some() || awake_changed);
    }
}

#[tauri::command]
#[specta::specta]
pub fn focus_state(focus: State<'_, Focus>) -> FocusSnapshot {
    snapshot(&focus)
}

#[tauri::command]
#[specta::specta]
pub fn focus_start(app: AppHandle, focus: State<'_, Focus>, mode: TimerMode, preset: TimerPreset) {
    lock(&focus.timer).start(mode, preset, now_ms());
    publish(&app, &focus, true);
}

#[tauri::command]
#[specta::specta]
pub fn focus_pause(app: AppHandle, focus: State<'_, Focus>) {
    lock(&focus.timer).pause(now_ms());
    publish(&app, &focus, true);
}

#[tauri::command]
#[specta::specta]
pub fn focus_resume(app: AppHandle, focus: State<'_, Focus>) {
    lock(&focus.timer).resume(now_ms());
    publish(&app, &focus, true);
}

#[tauri::command]
#[specta::specta]
pub fn focus_skip(app: AppHandle, focus: State<'_, Focus>) {
    lock(&focus.timer).skip();
    publish(&app, &focus, true);
}

/// Stops and clears. A stopwatch that ran a real while is kept for the streak.
#[tauri::command]
#[specta::specta]
pub fn focus_stop(app: AppHandle, focus: State<'_, Focus>) {
    let session = lock(&focus.timer).stop(now_ms());
    if let Some(session) = session {
        record(&focus, session);
    }
    publish(&app, &focus, true);
}

#[tauri::command]
#[specta::specta]
pub fn focus_stats(focus: State<'_, Focus>) -> Result<FocusStats, String> {
    let log = lock(&focus.log);
    let log = log.as_ref().ok_or("focus history is unavailable")?;
    log.stats(chrono::Local::now().date_naive())
        .map_err(|e| e.to_string())
}

/// Keeps the Mac (and its display) awake for `minutes`, or until stopped.
/// Off macOS there is no `caffeinate`, and the error says so.
#[tauri::command]
#[specta::specta]
pub fn awake_start(
    app: AppHandle,
    focus: State<'_, Focus>,
    minutes: Option<u32>,
) -> Result<(), String> {
    lock(&focus.awake)
        .start(minutes, now_ms())
        .map_err(|e| format!("could not run caffeinate: {e}"))?;
    publish(&app, &focus, true);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn awake_stop(app: AppHandle, focus: State<'_, Focus>) {
    lock(&focus.awake).stop();
    publish(&app, &focus, true);
}

/// The notch's own notes, newest first.
#[tauri::command]
#[specta::specta]
pub fn quick_notes() -> Result<Vec<aloud_notes::Note>, String> {
    let dir = aloud_notes::directory().map_err(|e| e.to_string())?;
    Ok(aloud_notes::quick_notes(&dir))
}

/// Saves a quick note, creating its file on the first call (`path` None) and
/// returning the path to pass on the next. The notes folder is the only place
/// it will write.
#[tauri::command]
#[specta::specta]
pub fn save_quick_note(path: Option<String>, text: String) -> Result<String, String> {
    let dir = aloud_notes::directory().map_err(|e| e.to_string())?;
    let path = path.map(std::path::PathBuf::from);
    aloud_notes::save_quick(&dir, path.as_deref(), &text, chrono::Utc::now())
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}
