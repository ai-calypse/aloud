//! Global hotkeys with hold, toggle, and modifier-only bindings.
//!
//! Ported from the Swift app's `ShortcutCore` + `GlobalShortcutBackend`.
//!
//! Tauri's `global-shortcut` plugin is deliberately not used: it cannot
//! express a modifier-only binding ("hold Right Option"), cannot distinguish
//! hold from tap, and cannot swallow the event before the focused app sees it.
//! All three are core to how this app dictates, so each OS gets a raw hook:
//! CGEventTap on macOS, WH_KEYBOARD_LL on Windows.
//!
//! The interesting logic lives in [`matcher`], which is pure and runs its
//! tests on any platform.

pub mod matcher;
pub mod model;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
use macos as backend;
#[cfg(target_os = "windows")]
use windows as backend;

use anyhow::Result;
use matcher::{Consume, Input, State};
use model::{Config, Event};
use std::sync::{Arc, Mutex};

/// Whether the OS will let us listen for keys.
///
/// macOS needs Accessibility permission; Windows has no equivalent gate.
pub fn is_permitted() -> bool {
    backend::is_trusted()
}

/// Listens for the configured hotkeys, calling `on_event` as roles activate
/// and deactivate. **Blocks forever** — both backends need to own a thread
/// (a run loop on macOS, a message pump on Windows), so run this on its own.
///
/// `config` is shared so settings changes apply without restarting the hook.
///
/// `busy` is set by the app while a session is still running after its key was
/// released. Escape only cancels — and is only swallowed — while a role is held
/// or `busy` is set, so Escape keeps working everywhere else.
pub fn listen<F>(
    config: Arc<Mutex<Config>>,
    busy: Arc<std::sync::atomic::AtomicBool>,
    mut on_event: F,
) -> Result<()>
where
    F: FnMut(Event) + Send + 'static,
{
    let mut state = State::default();

    let handler = move |raw: backend::RawEvent| -> bool {
        let input = if raw.down {
            Input::KeyDown(raw.key)
        } else {
            Input::KeyUp(raw.key)
        };

        state.set_busy(busy.load(std::sync::atomic::Ordering::Relaxed));

        // A poisoned lock must not wedge the keyboard: pass the event through.
        let Ok(config) = config.lock() else {
            tracing::error!("hotkey config lock poisoned");
            return false;
        };

        let outcome = state.reduce(input, &config);
        drop(config);

        for event in outcome.events {
            on_event(event);
        }
        outcome.consume == Consume::Yes
    };

    #[cfg(target_os = "macos")]
    return macos::run_tap(handler);
    #[cfg(target_os = "windows")]
    return windows::run_hook(handler);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = handler;
        anyhow::bail!("hotkeys are only supported on macOS and Windows")
    }
}
