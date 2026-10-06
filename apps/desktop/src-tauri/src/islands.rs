//! Wiring the island queue (`aloud-islands`) to the notch window.
//!
//! Orchestration only: one queue, an `island` event per change, the clock
//! that expires islands, and the two commands the webview needs. What gets
//! shown and in what order is the crate's.
//!
//! [`takeover`] is the one door for an event that wants the notch's attention:
//! the crate's gate and stage decide, and this performs what they say.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use aloud_islands::gate::TYPING_MS;
use aloud_islands::stage::{Action, Env, Stage};
use aloud_islands::{Island, IslandChange, IslandSource, IslandTier, Queue};
use aloud_store::settings::NotchEvents;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

/// One change to the queue, with the ranked order after it — so the webview
/// orders its copy by Rust's ranking rather than by a second one of its own.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "island")]
pub struct IslandChanged {
    pub change: IslandChange,
    pub order: Vec<String>,
}

/// How many takeovers are waiting for their turn: the "+N" the webview draws
/// on whatever is showing. Distinct items, never repeats.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "islands-waiting")]
pub struct IslandsWaiting {
    pub count: u32,
}

const LABEL: &str = "overlay";

static QUEUE: Mutex<Queue> = Mutex::new(Queue::new());

fn queue() -> std::sync::MutexGuard<'static, Queue> {
    // A panic elsewhere while holding it must not take every island with it.
    QUEUE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn send(app: &AppHandle, change: IslandChange, order: Vec<String>) {
    // Whoever took an island down — its owner, the user, the clock — the stage
    // must hear it, or the next takeover waits on a ghost.
    if let IslandChange::Remove(id) = &change {
        ended(app, id);
    }
    if let Err(e) = (IslandChanged { change, order }).emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send island");
    }
}

/// Adds or updates an island. A no-op push (same island, same state) sends
/// nothing — see `Queue::push`.
pub fn push(app: &AppHandle, island: Island) {
    let expires = island.expires;
    let sent = {
        let mut queue = queue();
        queue
            .push(island, Instant::now())
            .map(|change| (change, queue.order()))
    };
    if let Some((change, order)) = sent {
        send(app, change, order);
    }
    // A level held back by the rate limit still has to land: the trailing edge.
    if queue().has_held() {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(aloud_islands::LEVEL_GAP).await;
            let (changes, order) = {
                let mut queue = queue();
                (queue.release_held(Instant::now()), queue.order())
            };
            for change in changes {
                send(&app, change, order.clone());
            }
        });
    }
    if let Some(ms) = expires {
        // ponytail: a sleeping task per expiring push, not one clock on the
        // queue's next deadline. `expire` is idempotent and pushes are rare;
        // a single timer is the upgrade if something pushes many per second.
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(u64::from(ms))).await;
            let (gone, order) = {
                let mut queue = queue();
                let gone = queue.expire(Instant::now());
                (gone, queue.order())
            };
            for change in gone {
                send(&app, change, order.clone());
            }
        });
    }
}

/// Takes an island down, by id. Silent if it was already gone.
pub fn remove(app: &AppHandle, id: &str) {
    // Also when the queue never held it: a takeover still waiting for its turn
    // lives only in the stage.
    ended(app, id);
    let sent = {
        let mut queue = queue();
        queue.remove(id).map(|change| (change, queue.order()))
    };
    if let Some((change, order)) = sent {
        send(app, change, order);
    }
}

/// Everything on screen, ranked — for a webview that mounts after islands
/// were pushed (the event only carries changes).
#[tauri::command]
#[specta::specta]
pub fn islands() -> Vec<Island> {
    queue().ranked()
}

/// The user is done with an island: its ×, an alert's clock running out, or
/// an action that settles it. The action itself runs in the webview's
/// registry; this is the half of it the queue has to hear.
#[tauri::command]
#[specta::specta]
pub fn dismiss_island(app: AppHandle, id: String) {
    crate::mac::island_dismissed(&id);
    remove(&app, &id);
}

// ---- takeovers --------------------------------------------------------------

static STAGE: Mutex<Stage> = Mutex::new(Stage::new());
static TICKING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn stage() -> std::sync::MutexGuard<'static, Stage> {
    STAGE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An event that wants the notch's attention. The gate decides whether it
/// shows now, waits its turn or is not worth showing — Quiet mode, Focus, the
/// user typing and a panel being open all weigh in — and the stage holds the
/// one on screen and the line behind it.
pub fn takeover(app: &AppHandle, island: Island) {
    offer(app, island, false);
}

/// [`takeover`] from a run that owns the open panel, which the takeover
/// replaces in place rather than waiting for.
pub fn takeover_from_run(app: &AppHandle, island: Island) {
    offer(app, island, true);
}

fn offer(app: &AppHandle, island: Island, own_panel: bool) {
    let id = island.id.clone();
    let (decision, actions) = {
        let env = env(app, own_panel);
        stage().offer(island, &env, Instant::now())
    };
    tracing::debug!(%id, ?decision, "takeover");
    perform(app, actions, own_panel);
    ensure_ticker(app);
}

/// The webview or an owner is done with `id`: the next waiting takeover may
/// take the stage.
fn ended(app: &AppHandle, id: &str) {
    stage().end(id, Instant::now());
    ensure_ticker(app);
}

/// The user opened a panel over the notch. The takeover cannot outlive it: a
/// card waits for the panel to close, news is let go.
pub fn panel_opened(app: &AppHandle) {
    let actions = stage().panel_opened(Instant::now());
    perform(app, actions, false);
}

/// The panel closed; anything that waited for it may come back.
pub fn panel_closed(app: &AppHandle) {
    ensure_ticker(app);
}

fn perform(app: &AppHandle, actions: Vec<Action>, own_panel: bool) {
    for action in actions {
        match action {
            Action::Show(mut island) => {
                // A run's own panel gives way to the whisper that ends it: the
                // whisper claims no window, so nothing else would close it.
                if own_panel && island.tier == Some(IslandTier::Whisper) {
                    crate::overlay::end_panel(app);
                }
                // A whisper lives in the resting window and claims nothing; a
                // peek or card (and a shrunk one, which opens on hover) takes
                // the window the way a toast always has.
                if island.tier != Some(IslandTier::Whisper)
                    && !crate::overlay::show_toast(app, own_panel)
                {
                    tracing::warn!(id = %island.id, "no window for a takeover");
                    continue;
                }
                // The stage owns how long it lives; a queue lease would only
                // race it.
                island.expires = None;
                push(app, island);
            }
            Action::Side(mut island) => {
                island.expires = None;
                push(app, island);
            }
            Action::Clear(id) => remove(app, &id),
            Action::Waiting(count) => {
                let count = u32::try_from(count).unwrap_or(u32::MAX);
                if let Err(e) = (IslandsWaiting { count }).emit_to(app, LABEL) {
                    tracing::warn!(%e, "could not send waiting count");
                }
            }
        }
    }
}

/// Runs the stage's clock while there is anything on it, and stops when there
/// is not. Waking only for a takeover is the point: an idle app has no timer.
fn ensure_ticker(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    if TICKING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let (actions, active) = {
                let env = env(&app, false);
                let mut stage = stage();
                let actions = stage.tick(&env, Instant::now());
                (actions, stage.is_active())
            };
            perform(&app, actions, false);
            if !active {
                TICKING.store(false, Ordering::SeqCst);
                // An offer between the check and the store saw TICKING set and
                // spawned nothing: take the clock back for it.
                if stage().is_active() && !TICKING.swap(true, Ordering::SeqCst) {
                    continue;
                }
                return;
            }
        }
    });
}

/// What the gate needs to know right now.
fn env(app: &AppHandle, own_panel: bool) -> Env {
    let events = app
        .try_state::<crate::AppState>()
        .and_then(|state| state.settings.lock().ok().map(|s| s.notch_events.clone()))
        .unwrap_or_default();
    Env {
        panel_open: !own_panel && crate::overlay::user_panel_open(),
        typing: aloud_hotkey::since_last_keystroke()
            .is_some_and(|ago| ago < Duration::from_millis(TYPING_MS)),
        focus_on: events.focus_aware && focus_on(),
        quiet: events.quiet,
        muted: muted(&events),
    }
}

fn muted(events: &NotchEvents) -> Vec<IslandSource> {
    [
        (IslandSource::Hud, events.huds),
        (IslandSource::Power, events.power),
        (IslandSource::Connectivity, events.connectivity),
        (IslandSource::Banners, events.banners),
    ]
    .into_iter()
    .filter_map(|(source, on)| (!on).then_some(source))
    .collect()
}

/// Focus is read from a file, and the stage's clock asks every 200 ms, so the
/// answer is kept for a few seconds.
fn focus_on() -> bool {
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((at, on)) = *cache {
        if at.elapsed() < Duration::from_secs(5) {
            return on;
        }
    }
    let on = aloud_mac::focus_mode::is_on();
    *cache = Some((Instant::now(), on));
    on
}
