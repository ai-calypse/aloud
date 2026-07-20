//! Clipboard write, synthetic paste, and clipboard restore.
//!
//! Ported from the pasteboard half of `AppState.swift`. Dictation works by
//! putting text on the clipboard and synthesizing the paste chord, so the
//! user's existing clipboard has to be saved and put back afterwards.
//!
//! `enigo` replaces the hand-rolled `CGEvent` + `UCKeyTranslate` keycode
//! lookup: it resolves the physical key for a character against the active
//! layout on both platforms, which matters because "V" is not the same key on
//! QWERTY, Dvorak and AZERTY.

use anyhow::{Context, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::{thread::sleep, time::Duration};

/// How long to wait for the main thread to service a dispatched key chord.
/// Generous — the main thread only has to reach its run loop once — but
/// bounded, so a process without a main run loop (tests, examples) gets an
/// error instead of a hang.
#[cfg(target_os = "macos")]
const MAIN_THREAD_WAIT: Duration = Duration::from_secs(2);

/// Runs `f` on the main thread and waits for its result.
///
/// enigo's layout-dependent keycode lookup calls TIS/TSM
/// (`TISCopyCurrentKeyboardInputSource`), and modern macOS aborts the whole
/// process (`dispatch_assert_queue` → SIGTRAP) when those run off the main
/// queue. The pipeline pastes from a tokio worker and the rewrite read copies from
/// the event-tap thread, so every synthetic key chord must hop here first.
#[cfg(target_os = "macos")]
fn on_main_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
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
    // If the wait below times out and the block still runs later, the send
    // just fails on a closed channel — nothing dangles.
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
fn on_main_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    Ok(f())
}

/// Time for the target app to service the paste before the clipboard is
/// restored. Restoring too early pastes the *old* clipboard.
const PASTE_SETTLE: Duration = Duration::from_millis(150);

/// The modifier that pairs with V to paste on this platform.
const PASTE_MODIFIER: Key = if cfg!(target_os = "macos") {
    Key::Meta // Command
} else {
    Key::Control
};

/// A saved clipboard, restored on [`Snapshot::restore`].
///
/// ponytail: text and image only. The Swift version preserved every
/// pasteboard type, but `NSPasteboardItem`'s arbitrary-type model has no
/// cross-platform equivalent, and text/image covers effectively all real
/// clipboard content. Anything else is dropped — the alternative is
/// per-platform format enumeration for a rare case.
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
    /// Reads the current clipboard.
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

    /// Puts the saved contents back.
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
            // Nothing was there; leave whatever is there now rather than
            // clearing a clipboard we do not own.
            Snapshot::Empty => Ok(()),
        }
    }
}

/// Pastes `text` into the focused app.
///
/// `press_enter` sends Return afterwards — the Swift app's "press enter"
/// voice command.
///
/// `preserve_clipboard` puts the user's previous clipboard back once the paste
/// has settled. Turning it off leaves the dictated text on the clipboard, which
/// is what a user who wants to paste the same transcript again expects — the
/// Swift app exposed the same choice.
pub fn paste_text(text: &str, press_enter: bool, preserve_clipboard: bool) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context("opening clipboard")?;
    // Only worth reading the old clipboard if it is going back afterwards.
    let saved = preserve_clipboard.then(|| Snapshot::capture(&mut clipboard));

    clipboard.set_text(text).context("writing transcript")?;
    let result = send_paste(press_enter);

    // Restore even if the paste failed, so a failure never eats the clipboard.
    //
    // On its own thread, because the settle wait is dead time the *user* was
    // being charged for: the text is already in their document by here, and
    // waiting inline delayed the pill's "Pasted" by the full 150ms and parked a
    // tokio worker for the duration. Nothing downstream reads the clipboard, so
    // there is nothing to order against.
    if let Some(saved) = saved {
        std::thread::spawn(move || {
            sleep(PASTE_SETTLE);
            // A second handle: `clipboard` stays with the caller, and arboard's
            // is not Send on every platform.
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

/// Puts `text` on the clipboard and leaves it there — no paste, no restore.
///
/// This is for the tray's history menu: a menu closes by moving focus, so
/// synthesising a paste chord would land in whatever happened to be in front.
/// Copying and letting the user paste is both safer and what the Swift menu did.
pub fn set_clipboard(text: &str) -> Result<()> {
    arboard::Clipboard::new()
        .context("opening clipboard")?
        .set_text(text)
        .context("writing to clipboard")
}

/// Reads the clipboard as text. An image or an empty board yields an error
/// rather than a blank string, so a caller cannot mistake "nothing there" for
/// "the user copied whitespace".
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
        // Always release the modifier: leaving Cmd stuck down would wedge the
        // user's keyboard.
        enigo.key(PASTE_MODIFIER, Direction::Release)?;
        pressed.context("sending paste")?;

        if press_enter {
            enigo
                .key(Key::Return, Direction::Click)
                .context("sending return")?;
        }
        Ok(())
    })?
}

/// Copies the current selection and returns it, restoring the clipboard.
///
/// The cross-platform fallback for reading selected text: macOS can do this
/// properly through the Accessibility API, but Windows UI Automation is
/// unreliable across apps, so this synthesizes the copy chord instead.
pub fn copy_selection() -> Result<String> {
    let mut clipboard = arboard::Clipboard::new().context("opening clipboard")?;
    let saved = Snapshot::capture(&mut clipboard);

    // Clear first: otherwise a copy that matches the existing clipboard, or
    // one that silently fails, is indistinguishable from a real result.
    let sentinel = "\u{0}aloud-selection-probe";
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
    saved.restore(&mut clipboard).ok();

    if selection == sentinel {
        return Ok(String::new()); // nothing was selected
    }
    Ok(selection)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips through the real clipboard: capture must return what was
    /// there, and restore must put it back after an overwrite.
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
    fn paste_modifier_matches_platform() {
        if cfg!(target_os = "macos") {
            assert_eq!(PASTE_MODIFIER, Key::Meta);
        } else {
            assert_eq!(PASTE_MODIFIER, Key::Control);
        }
    }
}
