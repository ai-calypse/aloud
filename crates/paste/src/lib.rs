//! Clipboard write, synthetic paste, and clipboard restore.
//!
//! Dictation puts text on the clipboard and synthesizes the paste chord, so the
//! user's clipboard has to be saved and put back afterwards.
//!
//! `enigo` rather than a hand-rolled `CGEvent` keycode lookup: it resolves the
//! physical key against the active layout, and "V" is not the same key on
//! QWERTY, Dvorak and AZERTY.

use anyhow::{Context, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::{thread::sleep, time::Duration};

/// When Aloud last wrote the clipboard itself. A clipboard history watching
/// `changeCount` cannot tell our dictation paste (and the restore after it)
/// from something the user copied; this is how it keeps Aloud's own traffic
/// out of the history. A time window rather than a write count: each backend
/// bumps `changeCount` a different number of times per write.
static OWN_WRITE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

fn note_own_write() {
    *OWN_WRITE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(std::time::Instant::now());
}

/// Whether Aloud wrote the clipboard within `window` — the caller treats any
/// change seen in that span as ours.
pub fn wrote_within(window: Duration) -> bool {
    OWN_WRITE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_some_and(|at| at.elapsed() < window)
}

/// Bounded so a process with no main run loop (tests, examples) errors rather
/// than hangs.
#[cfg(target_os = "macos")]
const MAIN_THREAD_WAIT: Duration = Duration::from_secs(2);

/// Runs `f` on the main thread and waits for its result.
///
/// enigo's `Key::Unicode` lookup calls TIS/TSM, and modern macOS *aborts the
/// process* (`dispatch_assert_queue` → SIGTRAP) when those run off the main
/// queue. Every synthetic key chord must hop here first — pasting happens on a
/// tokio worker, the rewrite read on the event-tap thread.
#[cfg(target_os = "macos")]
pub fn on_main_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    use std::ffi::c_void;

    #[repr(C)]
    struct DispatchQueue {
        _private: [u8; 0],
    }
    extern "C" {
        // `dispatch_get_main_queue()` is a C macro over this static.
        static _dispatch_main_q: DispatchQueue;
        fn dispatch_async_f(
            queue: *const DispatchQueue,
            context: *mut c_void,
            work: extern "C" fn(*mut c_void),
        );
        fn pthread_main_np() -> i32;
    }

    extern "C" fn trampoline(context: *mut c_void) {
        // SAFETY: `context` is the Box leaked below; this runs exactly once.
        let f = unsafe { Box::from_raw(context.cast::<Box<dyn FnOnce() + Send>>()) };
        f();
    }

    if unsafe { pthread_main_np() } != 0 {
        return Ok(f());
    }

    let (tx, rx) = std::sync::mpsc::channel();
    // A late-running block after a timeout just fails to send; nothing dangles.
    let block: Box<dyn FnOnce() + Send> = Box::new(move || {
        let _ = tx.send(f());
    });
    let context = Box::into_raw(Box::new(block));
    unsafe {
        dispatch_async_f(&_dispatch_main_q, context.cast::<c_void>(), trampoline);
    }
    rx.recv_timeout(MAIN_THREAD_WAIT)
        .context("main thread did not run the key chord (no run loop?)")
}

#[cfg(not(target_os = "macos"))]
pub fn on_main_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    Ok(f())
}

/// Time for the target app to service the paste. Restoring earlier pastes the
/// *old* clipboard.
const PASTE_SETTLE: Duration = Duration::from_millis(150);
/// Between a paste and the spoken "press enter"'s Return. A calibration knob,
/// not a derived number: raise it if a chat app submits before the paste lands.
const SUBMIT_DELAY: Duration = Duration::from_millis(120);

/// The modifier that pairs with V to paste on this platform.
const PASTE_MODIFIER: Key = if cfg!(target_os = "macos") {
    Key::Meta // Command
} else {
    Key::Control
};

/// A saved clipboard, restored on [`Snapshot::restore`].
///
/// ponytail: text and image only — anything else is dropped. Per-platform
/// format enumeration is the upgrade path if a real case turns up.
#[derive(Debug, Default)]
pub enum Snapshot {
    Text(String),
    Image {
        width: usize,
        height: usize,
        bytes: Vec<u8>,
    },
    #[default]
    Empty,
}

impl Snapshot {
    pub fn capture(clipboard: &mut arboard::Clipboard) -> Self {
        if let Ok(text) = clipboard.get_text() {
            return Snapshot::Text(text);
        }
        match clipboard.get_image() {
            Ok(image) => Snapshot::Image {
                width: image.width,
                height: image.height,
                bytes: image.bytes.into_owned(),
            },
            Err(_) => Snapshot::Empty,
        }
    }

    pub fn restore(self, clipboard: &mut arboard::Clipboard) -> Result<()> {
        match self {
            Snapshot::Text(text) => clipboard.set_text(text).context("restoring clipboard text"),
            Snapshot::Image {
                width,
                height,
                bytes,
            } => clipboard
                .set_image(arboard::ImageData {
                    width,
                    height,
                    bytes: bytes.into(),
                })
                .context("restoring clipboard image"),
            // Nothing was there; do not clear a clipboard we do not own.
            Snapshot::Empty => Ok(()),
        }
    }
}

/// Pastes `text` into the focused app, optionally sending Return afterwards and
/// optionally putting the user's previous clipboard back once it has settled.
pub fn paste_text(text: &str, press_enter: bool, preserve_clipboard: bool) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context("opening clipboard")?;
    // Only worth reading the old clipboard if it is going back afterwards.
    let saved = preserve_clipboard.then(|| Snapshot::capture(&mut clipboard));

    note_own_write();
    clipboard.set_text(text).context("writing transcript")?;
    note_own_write();
    let result = send_paste(press_enter);

    // Restore even if the paste failed, so a failure never eats the clipboard.
    // On its own thread: the text is already in the document by here, and
    // waiting inline charged the user the full settle before "Pasted".
    if let Some(saved) = saved {
        std::thread::spawn(move || {
            sleep(PASTE_SETTLE);
            note_own_write();
            // A second handle: arboard's is not Send on every platform.
            match arboard::Clipboard::new() {
                Ok(mut clipboard) => {
                    if let Err(e) = saved.restore(&mut clipboard) {
                        tracing::warn!(%e, "could not restore clipboard");
                    }
                }
                Err(e) => tracing::warn!(%e, "could not reopen clipboard to restore"),
            }
        });
    }
    result
}

/// Puts `text` on the clipboard and leaves it there — no paste, no restore. For
/// the tray's history menu: a menu closes by moving focus, so a synthesised
/// paste would land in whatever happened to be in front.
pub fn set_clipboard(text: &str) -> Result<()> {
    note_own_write();
    arboard::Clipboard::new()
        .context("opening clipboard")?
        .set_text(text)
        .context("writing to clipboard")
}

/// Puts an RGBA image on the clipboard and leaves it there, like [`set_clipboard`].
pub fn set_clipboard_image(width: usize, height: usize, rgba: Vec<u8>) -> Result<()> {
    note_own_write();
    arboard::Clipboard::new()
        .context("opening clipboard")?
        .set_image(arboard::ImageData {
            width,
            height,
            bytes: rgba.into(),
        })
        .context("writing image to clipboard")
}

/// Sends the paste chord for whatever is on the clipboard now. For a caller
/// that already put its content there (a history entry, a snippet) and has
/// handed focus back.
pub fn paste_clipboard() -> Result<()> {
    send_paste(false)
}

/// Reads the clipboard as text. An image or empty board errors rather than
/// returning a blank string, so "nothing there" is distinguishable.
pub fn clipboard_text() -> Result<String> {
    arboard::Clipboard::new()
        .context("opening clipboard")?
        .get_text()
        .context("reading clipboard")
}

/// Sends the paste chord (Cmd+V / Ctrl+V), optionally followed by Return.
fn send_paste(press_enter: bool) -> Result<()> {
    on_main_thread(move || -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default()).context("creating input simulator")?;

        enigo.key(PASTE_MODIFIER, Direction::Press)?;
        let pressed = enigo.key(Key::Unicode('v'), Direction::Click);
        // Always release: a stuck Cmd wedges the user's keyboard.
        enigo.key(PASTE_MODIFIER, Direction::Release)?;
        pressed.context("sending paste")
    })??;

    if press_enter {
        // Off the main thread: Electron apps (Slack, Discord) apply a paste
        // asynchronously, and a Return on its heels submits before the text
        // lands.
        sleep(SUBMIT_DELAY);
        on_main_thread(|| -> Result<()> {
            let mut enigo = Enigo::new(&Settings::default()).context("creating input simulator")?;
            enigo
                .key(Key::Return, Direction::Click)
                .context("sending return")
        })??;
    }
    Ok(())
}

/// A modifier held for [`press_chord`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Command,
    Shift,
    Option,
    Control,
}

/// The key of a chord: a character, resolved against the active layout so ⌘Z
/// is the key that types "z" on AZERTY too, or a raw virtual key code for keys
/// with no character (Return, Escape, F5, the arrows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordKey {
    Char(char),
    Code(u16),
}

/// Presses one chord as a person would — modifiers down, the key, modifiers up
/// — through the same main-thread CGEvent path as the paste chord. Posted
/// events need only Accessibility; System Events' `keystroke` also needs an
/// Automation grant and adds an Apple Event round trip.
pub fn press_chord(modifiers: &[Modifier], key: ChordKey) -> Result<()> {
    // A password field or Terminal's Secure Keyboard Entry drops synthetic
    // keys without a word; saying so beats a shortcut that silently did nothing.
    #[cfg(target_os = "macos")]
    {
        #[link(name = "Carbon", kind = "framework")]
        extern "C" {
            fn IsSecureEventInputEnabled() -> bool;
        }
        // SAFETY: a read-only query with no arguments.
        if unsafe { IsSecureEventInputEnabled() } {
            anyhow::bail!(
                "secure keyboard input is on (a password field or Terminal's Secure Keyboard \
                 Entry), which blocks pressed keys"
            );
        }
    }
    let modifiers = modifiers.to_vec();
    on_main_thread(move || -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default()).context("creating input simulator")?;
        let held: Vec<Key> = modifiers
            .iter()
            .map(|m| match m {
                Modifier::Command => Key::Meta,
                Modifier::Shift => Key::Shift,
                Modifier::Option => Key::Alt,
                Modifier::Control => Key::Control,
            })
            .collect();
        for k in &held {
            enigo.key(*k, Direction::Press)?;
        }
        let pressed = enigo.key(
            match key {
                ChordKey::Char(c) => match main_row(c) {
                    Some(code) => Key::Other(u32::from(code)),
                    None => Key::Unicode(c),
                },
                ChordKey::Code(code) => Key::Other(u32::from(code)),
            },
            Direction::Click,
        );
        // Always release, in reverse: a stuck Cmd wedges the user's keyboard.
        for k in held.iter().rev() {
            let _ = enigo.key(*k, Direction::Release);
        }
        pressed.context("pressing the key")?;
        Ok(())
    })?
}

/// The main-row key for characters the numeric keypad also types. enigo finds
/// a character's key by scanning every key, and for "5" it lands on keypad 5 —
/// the same character, but ⌘⇧keypad-5 is not the screenshot shortcut, which
/// macOS matches on the key, not the character. The digit row is the same key
/// on every common layout. Letters have no keypad twin and keep going through
/// the layout, which is what app menu shortcuts match.
fn main_row(c: char) -> Option<u16> {
    // kVK_ANSI_* codes.
    Some(match c {
        '1' => 18,
        '2' => 19,
        '3' => 20,
        '4' => 21,
        '5' => 23,
        '6' => 22,
        '7' => 26,
        '8' => 28,
        '9' => 25,
        '0' => 29,
        '-' => 27,
        '=' => 24,
        '/' => 44,
        '.' => 47,
        _ => return None,
    })
}

/// Copies the current selection and returns it, restoring the clipboard. The
/// cross-platform fallback for reading selected text.
pub fn copy_selection() -> Result<String> {
    let mut clipboard = arboard::Clipboard::new().context("opening clipboard")?;
    let saved = Snapshot::capture(&mut clipboard);

    // Sentinel first: a copy that silently failed, or that matches the existing
    // clipboard, is otherwise indistinguishable from a real result.
    let sentinel = "\u{0}aloud-selection-probe";
    note_own_write();
    clipboard.set_text(sentinel).ok();

    on_main_thread(|| -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default()).context("creating input simulator")?;
        enigo.key(PASTE_MODIFIER, Direction::Press)?;
        let copied = enigo.key(Key::Unicode('c'), Direction::Click);
        enigo.key(PASTE_MODIFIER, Direction::Release)?;
        copied.context("sending copy")
    })??;

    sleep(PASTE_SETTLE);
    let selection = clipboard.get_text().unwrap_or_default();
    note_own_write();
    saved.restore(&mut clipboard).ok();
    note_own_write();

    if selection == sentinel {
        return Ok(String::new()); // nothing was selected
    }
    Ok(selection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_restores_previous_text() {
        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            eprintln!("no clipboard available; skipping");
            return;
        };

        clipboard.set_text("original").unwrap();
        let saved = Snapshot::capture(&mut clipboard);

        clipboard.set_text("transcript").unwrap();
        assert_eq!(clipboard.get_text().unwrap(), "transcript");

        saved.restore(&mut clipboard).unwrap();
        assert_eq!(clipboard.get_text().unwrap(), "original");
    }

    #[test]
    fn an_own_write_is_remembered_only_for_its_window() {
        // The clipboard history relies on this to skip Aloud's own paste and
        // restore; a window that never closes would hide the user's copies.
        note_own_write();
        assert!(wrote_within(Duration::from_secs(5)));
        assert!(!wrote_within(Duration::ZERO));
    }

    #[test]
    fn paste_modifier_matches_platform() {
        if cfg!(target_os = "macos") {
            assert_eq!(PASTE_MODIFIER, Key::Meta);
        } else {
            assert_eq!(PASTE_MODIFIER, Key::Control);
        }
    }
}
