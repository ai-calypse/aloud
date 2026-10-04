//! Wiring the island queue (`aloud-islands`) to the notch window.
//!
//! Orchestration only: one queue, an `island` event per change, the clock
//! that expires islands, and the two commands the webview needs. What gets
//! shown and in what order is the crate's.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use aloud_islands::{Island, IslandChange, Queue};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
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

const LABEL: &str = "overlay";

static QUEUE: Mutex<Queue> = Mutex::new(Queue::new());

fn queue() -> std::sync::MutexGuard<'static, Queue> {
    // A panic elsewhere while holding it must not take every island with it.
    QUEUE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn send(app: &AppHandle, change: IslandChange, order: Vec<String>) {
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
