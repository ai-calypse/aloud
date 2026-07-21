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

/// One armed recording: where to send the gesture, and which request it is.
///
/// The id exists so a request that gives up can tell whether the slot is still
/// its own. Retargeting to a second row drops the first sender, which wakes
/// its task as if it had timed out — and clearing then would disarm the
/// recording that just replaced it.
pub struct Capture {
    pub id: u64,
    pub tx: std::sync::mpsc::Sender<Binding>,
}

/// Shared slot that arms binding capture from another thread.
///
/// `Some(..)` puts the listener into capture mode: the next complete key
/// gesture is sent back and every event is swallowed until it commits. The
/// listener clears the slot itself, so capture is always one-shot — a UI that
/// crashes mid-recording cannot leave the keyboard permanently swallowed.
pub type CaptureSlot = Arc<Mutex<Option<Capture>>>;

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
/// `on_event` also receives the modifiers held when the event fired — the only
/// point at which that key state still exists.
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
    // Capture swallows every key while armed, key-ups included, so anything the
    // user was holding when they clicked Record stays `pressed` in the matcher
    // forever. One phantom modifier fails the exact-match rule on every
    // modifier-only binding, and the dictate key is dead until relaunch.
    let mut was_armed = false;

    let handler = move |raw: Option<backend::RawEvent>| -> bool {
        // Capture runs ahead of the matcher and swallows everything: recording
        // a new hotkey must not also fire the hotkey being replaced, and the
        // keys pressed to record it must not reach the app underneath.
        //
        // A key with no `Key` variant — a digit, Return, an arrow — still has
        // to be swallowed while armed, even though it can never be recorded.
        let Some(raw) = raw else {
            was_armed |= capture_poison_if_armed(&capture, &mut recorder);
            return was_armed;
        };
        if capture_event(&capture, &mut recorder, raw.key, raw.down) {
            was_armed = true;
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

        // Resync on the armed→disarmed edge, before this event is reduced, and
        // forward what it tears down: a hold whose key-up capture ate is still
        // active, and the app is still recording against it.
        if std::mem::take(&mut was_armed) {
            let out = state.reduce(Input::Reset, &config);
            if !out.events.is_empty() {
                let held = state.modifiers();
                for event in out.events {
                    on_event(event, &held);
                }
            }
        }

        let outcome = state.reduce(input, &config);
        drop(config);

        // Only when something fired: this allocates a set, and it runs on the
        // event tap for every keystroke on the machine, almost none of which
        // produce an event. Read once, after the reduce, so every event from
        // this input sees the same key state.
        if !outcome.events.is_empty() {
            let held = state.modifiers();
            for event in outcome.events {
                on_event(event, &held);
            }
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

/// Discards the in-flight gesture when a key that cannot be recorded arrives,
/// and reports whether it was swallowed.
///
/// The failure this prevents: `key_from_keycode` has no mapping for digits,
/// Return, Tab or punctuation, so recording Cmd+Return dropped the Return and
/// still committed on the Command key-up — binding a bare "hold Command",
/// which then fires a dictation on every plain Cmd press. Poisoning the
/// gesture means nothing commits and the row keeps asking.
fn capture_poison_if_armed(capture: &CaptureSlot, recorder: &mut capture::Recorder) -> bool {
    let Ok(slot) = capture.lock() else {
        tracing::error!("hotkey capture lock poisoned");
        return false;
    };
    if slot.is_none() {
        return false;
    }
    recorder.reset();
    true
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

    // Escape is the bail-out, not a recordable key. Two problems, one guard:
    // capture swallows the whole keyboard while armed — Cmd+Tab and Cmd+Q
    // included — so without this the only ways out are the mouse and a
    // ten-second timeout; and the matcher handles Escape ahead of all binding
    // matching, so a binding containing it cancels its own session on the first
    // auto-repeat and can never fire. Keyed on the key-down alone, which also
    // blocks Cmd+Escape — equally broken, since that branch ignores modifiers.
    if down && key == model::Key::Escape {
        recorder.reset();
        // Dropping the sender is what wakes `capture_hotkey` with None.
        *slot = None;
        return true;
    }

    if down {
        recorder.key_down(key);
        return true;
    }
    if let Some(binding) = recorder.key_up(key) {
        // Take the sender before sending: capture is one-shot, and a UI that
        // has gone away just drops the value.
        if let Some(capture) = slot.take() {
            let _ = capture.tx.send(binding);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::Key;

    fn armed() -> (CaptureSlot, std::sync::mpsc::Receiver<Binding>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Arc::new(Mutex::new(Some(Capture { id: 0, tx }))), rx)
    }

    /// The failure: while capture is armed every key is swallowed, so Escape
    /// recorded *as a binding* instead of cancelling — and there was no
    /// keyboard way out at all, since Cmd+Tab and Cmd+Q are swallowed too.
    /// An Escape binding could never fire anyway: the matcher handles Escape
    /// ahead of all binding matching, so it cancels its own session.
    #[test]
    fn escape_cancels_capture_instead_of_recording() {
        let (capture, rx) = armed();
        let mut recorder = capture::Recorder::default();

        assert!(capture_event(&capture, &mut recorder, Key::Escape, true));
        assert!(
            capture.lock().unwrap().is_none(),
            "Escape must disarm the capture"
        );
        assert!(
            matches!(
                rx.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Disconnected)
            ),
            "the dropped sender is what wakes capture_hotkey with None"
        );
    }

    /// The failure: `key_from_keycode` has no variant for Return, so recording
    /// Cmd+Return dropped the Return and still committed on the Command
    /// key-up — silently binding "hold Command", which then fires a dictation
    /// on every plain Cmd press.
    #[test]
    fn an_unrecordable_key_commits_nothing() {
        let (capture, rx) = armed();
        let mut recorder = capture::Recorder::default();

        capture_event(&capture, &mut recorder, Key::LeftCommand, true);
        // Return has no `Key`, so it arrives as None and poisons the gesture.
        assert!(capture_poison_if_armed(&capture, &mut recorder));
        capture_event(&capture, &mut recorder, Key::LeftCommand, false);

        assert!(
            rx.try_recv().is_err(),
            "a gesture containing an unrecordable key must bind nothing"
        );
        assert!(
            capture.lock().unwrap().is_some(),
            "and the row keeps asking rather than committing garbage"
        );
    }
}
