//! Global hotkeys with hold, toggle, and modifier-only bindings.
//!
//! Tauri's `global-shortcut` plugin cannot express a modifier-only binding,
//! distinguish hold from tap, or swallow the event before the focused app sees
//! it — all three are core here, so each OS gets a raw hook: CGEventTap on
//! macOS, WH_KEYBOARD_LL on Windows. The logic lives in the pure [`matcher`].

pub mod aux;
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

/// No hook elsewhere — just enough for the workspace to build and its tests to
/// run on Linux (CI, a cloud dev box). `listen` reports it and returns.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod backend {
    // Read by `listen`'s handler, never built: no hook ever delivers one.
    #[allow(dead_code)]
    pub struct RawEvent {
        pub key: crate::model::Key,
        pub down: bool,
    }

    /// True, so the listener reaches `listen`'s error at once rather than
    /// polling forever for a permission that does not exist here.
    pub fn is_trusted() -> bool {
        true
    }
}

use anyhow::Result;
use matcher::{Consume, Input, State};
use model::{Binding, Config, Event};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// When the last key that types something went down, as milliseconds past
/// [`epoch`] plus one — zero is "never". Only a time: no key, no text.
static LAST_KEYSTROKE: AtomicU64 = AtomicU64::new(0);

/// A fixed origin for [`LAST_KEYSTROKE`], since an `Instant` does not fit an atomic.
fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// How long ago the user last pressed a key that types, or None if they have
/// not since launch. The notch reads it to keep a takeover off their text; it
/// costs one atomic load.
pub fn since_last_keystroke() -> Option<Duration> {
    let stamp = LAST_KEYSTROKE.load(Ordering::Relaxed).checked_sub(1)?;
    Some(
        epoch()
            .elapsed()
            .saturating_sub(Duration::from_millis(stamp)),
    )
}

/// Whether this event is the user typing. A bare modifier is not: holding Fn
/// to dictate must not look like typing, or the dictation's own result would
/// be pushed off the screen. An unmapped key (digits, Return, punctuation)
/// arrives as None and is typing.
fn is_typing(raw: Option<(model::Key, bool)>) -> bool {
    match raw {
        None => true,
        Some((key, down)) => down && key.logical_modifier().is_none(),
    }
}

fn note_keystroke() {
    let stamp = u64::try_from(epoch().elapsed().as_millis()).unwrap_or(u64::MAX - 1);
    LAST_KEYSTROKE.store(stamp + 1, Ordering::Relaxed);
}

/// One armed recording: where to send the gesture, and which request it is.
///
/// The id lets a request that gives up tell whether the slot is still its own —
/// retargeting drops the first sender, which wakes its task as if it had timed
/// out, and clearing then would disarm the recording that replaced it.
pub struct Capture {
    pub id: u64,
    pub tx: std::sync::mpsc::Sender<Binding>,
}

/// Shared slot that arms binding capture from another thread. `Some(..)` sends
/// back the next complete gesture and swallows everything until it commits. The
/// listener clears the slot itself, so a UI that crashes mid-recording cannot
/// leave the keyboard permanently swallowed.
pub type CaptureSlot = Arc<Mutex<Option<Capture>>>;

/// Whether the OS will let us listen for keys. macOS needs Accessibility;
/// Windows has no equivalent gate.
pub fn is_permitted() -> bool {
    backend::is_trusted()
}

/// Listens for the configured hotkeys, calling `on_event` as roles activate
/// and deactivate. **Blocks forever** — both backends need to own a thread
/// (a run loop on macOS, a message pump on Windows), so run this on its own.
///
/// `config` is shared so settings changes apply without restarting the hook.
/// `busy` marks a session still running after its key was released; Escape only
/// cancels while a role is held or `busy` is set, so it keeps working
/// elsewhere. `on_event` gets the modifiers held when the event fired — the
/// only point at which that key state still exists.
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
    // Capture swallows key-ups too, so a key held when Record was clicked stays
    // `pressed` in the matcher forever — one phantom modifier fails the
    // exact-match rule and the dictate key is dead until relaunch.
    let mut was_armed = false;

    let handler = move |raw: Option<backend::RawEvent>| -> bool {
        // Capture runs ahead of the matcher and swallows everything, unmapped
        // keys included: recording a hotkey must not fire the one it replaces,
        // nor let the keys pressed reach the app underneath.
        if is_typing(raw.as_ref().map(|r| (r.key, r.down))) {
            note_keystroke();
        }
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

        // Only when something fired: this allocates, and runs on the event tap
        // for every keystroke on the machine. Read once, after the reduce, so
        // every event from this input sees the same key state.
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

/// Discards the in-flight gesture when an unmappable key arrives (digits,
/// Return, punctuation), and reports whether it was swallowed. Without this,
/// recording Cmd+Return dropped the Return and committed a bare "hold Command"
/// that then dictates on every plain Cmd press.
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

/// Feeds one raw event to the capture recorder, returning whether capture
/// consumed it. Disarms itself the moment a gesture commits, so a settings
/// window that dies mid-recording cannot leave the keyboard swallowed.
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

    // Escape is the bail-out, not a recordable key: capture swallows Cmd+Tab and
    // Cmd+Q too, so it is the only keyboard way out, and the matcher handles
    // Escape ahead of binding matching so an Escape binding could never fire.
    // Keyed on the key-down alone, which also blocks the equally broken
    // Cmd+Escape.
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
        // Take the sender: capture is one-shot, and a UI that has gone away
        // just drops the value.
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

    /// The failure this guards: counting the dictation key as typing, so a
    /// result pasted right after a hold was treated as arriving under the
    /// user's hands and shrunk.
    #[test]
    fn a_modifier_is_not_typing_but_a_letter_and_an_unmapped_key_are() {
        assert!(!is_typing(Some((Key::Fn, true))));
        assert!(!is_typing(Some((Key::LeftCommand, true))));
        assert!(is_typing(Some((Key::Char('a'), true))));
        assert!(
            !is_typing(Some((Key::Char('a'), false))),
            "a release is not a press"
        );
        assert!(is_typing(None));
    }

    #[test]
    fn a_keystroke_is_timed_and_nothing_else() {
        note_keystroke();
        let ago = since_last_keystroke().expect("one was just noted");
        assert!(ago < Duration::from_secs(5));
    }

    fn armed() -> (CaptureSlot, std::sync::mpsc::Receiver<Binding>) {
        let (tx, rx) = std::sync::mpsc::channel();
        (Arc::new(Mutex::new(Some(Capture { id: 0, tx }))), rx)
    }

    /// Regression: Escape recorded *as a binding* instead of cancelling, leaving
    /// no keyboard way out — Cmd+Tab and Cmd+Q are swallowed too.
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

    /// Regression: recording Cmd+Return dropped the unmappable Return and
    /// committed "hold Command", dictating on every plain Cmd press.
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
