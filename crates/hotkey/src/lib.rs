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

pub mod capture;
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
use model::{Binding, Config, Event};
use std::sync::{Arc, Mutex};

/// Shared slot that arms binding capture from another thread.
///
/// `Some(sender)` puts the listener into capture mode: the next complete key
/// gesture is sent back and every event is swallowed until it commits. The
/// listener clears the slot itself, so capture is always one-shot — a UI that
/// crashes mid-recording cannot leave the keyboard permanently swallowed.
pub type CaptureSlot = Arc<Mutex<Option<std::sync::mpsc::Sender<Binding>>>>;

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
///
/// `on_event` also receives the modifiers held when the event fired, which is
/// the only place manual command mode can read them.
pub fn listen<F>(
    config: Arc<Mutex<Config>>,
    busy: Arc<std::sync::atomic::AtomicBool>,
    capture: CaptureSlot,
    mut on_event: F,
) -> Result<()>
where
    F: FnMut(Event, &model::Modifiers) + Send + 'static,
{
    let mut state = State::default();
    let mut recorder = capture::Recorder::default();

    let handler = move |raw: backend::RawEvent| -> bool {
        // Capture runs ahead of the matcher and swallows everything: recording
        // a new hotkey must not also fire the hotkey being replaced, and the
        // keys pressed to record it must not reach the app underneath.
        if capture_event(&capture, &mut recorder, raw.key, raw.down) {
            return true;
        }

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

        // Read once, after the reduce, so every event from this input sees the
        // same key state. Manual command mode is decided from it: the extra
        // modifier is held at the moment the dictate key activates, and there
        // is no later point at which it can still be observed.
        let held = state.held_modifiers();
        for event in outcome.events {
            on_event(event, &held);
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

/// Feeds one raw event to the capture recorder while capture is armed.
///
/// Returns whether the event was consumed by capture. Disarms itself the moment
/// a gesture commits, so the caller never has to — a settings window that dies
/// mid-recording cannot leave the keyboard swallowed.
fn capture_event(
    capture: &CaptureSlot,
    recorder: &mut capture::Recorder,
    key: model::Key,
    down: bool,
) -> bool {
    // A poisoned lock must not wedge the keyboard: fall through to the matcher.
    let Ok(mut slot) = capture.lock() else {
        tracing::error!("hotkey capture lock poisoned");
        return false;
    };
    if slot.is_none() {
        // Not capturing. Clear any half-finished gesture so keys held when
        // capture was cancelled cannot become modifiers on the next recording.
        recorder.reset();
        return false;
    }

    if down {
        recorder.key_down(key);
        return true;
    }
    if let Some(binding) = recorder.key_up(key) {
        // Take the sender before sending: capture is one-shot, and a UI that
        // has gone away just drops the value.
        if let Some(tx) = slot.take() {
            let _ = tx.send(binding);
        }
    }
    true
}
