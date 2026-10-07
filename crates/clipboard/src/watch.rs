//! Noticing a copy. macOS offers no pasteboard notification, so this polls
//! `changeCount` — an integer read, cheap enough at 500 ms — and only looks at
//! the contents when it moved.

use std::sync::Arc;
use std::time::Duration;

use crate::entry::Captured;
use crate::store::Store;

const POLL: Duration = Duration::from_millis(500);
/// How long after Aloud's own clipboard write a change is still ours. Covers
/// the paste, the 150 ms settle, the restore, and one poll to notice them.
const OWN_WRITE_WINDOW: Duration = Duration::from_millis(1500);

/// Reads whatever the clipboard holds now. Files beat text (Finder puts the
/// file name beside the file) and text beats an image (Excel and Pages put a
/// rendered picture beside the cells).
fn read_clipboard() -> Option<Captured> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    if let Ok(files) = clipboard.get().file_list() {
        let files: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if !files.is_empty() {
            return Some(Captured::Files(files));
        }
    }
    if let Ok(text) = clipboard.get_text() {
        if !text.trim().is_empty() {
            return Some(Captured::Text(text));
        }
    }
    let image = clipboard.get_image().ok()?;
    Some(Captured::Image {
        width: image.width,
        height: image.height,
        rgba: image.bytes.into_owned(),
    })
}

/// Starts the watcher thread. `on_change` runs after an entry was stored or
/// bumped, so the panel can refresh. `active` is the app's own off switch on
/// top of the store's (the notch being off, in Aloud): while it says no,
/// nothing is recorded. The thread also tracks `changeCount` while history is
/// off, so turning it on never swallows what was already on the clipboard.
#[cfg(target_os = "macos")]
pub fn spawn(
    store: Arc<Store>,
    active: impl Fn() -> bool + Send + 'static,
    on_change: impl Fn() + Send + 'static,
) {
    use objc2_app_kit::NSPasteboard;

    std::thread::spawn(move || {
        let pasteboard = NSPasteboard::generalPasteboard();
        let mut seen = pasteboard.changeCount();
        loop {
            std::thread::sleep(POLL);
            let count = pasteboard.changeCount();
            if count == seen {
                continue;
            }
            seen = count;
            if !active() || !store.enabled() || aloud_paste::wrote_within(OWN_WRITE_WINDOW) {
                continue;
            }
            let types: Vec<String> = pasteboard
                .types()
                .map(|types| types.iter().map(|t| t.to_string()).collect())
                .unwrap_or_default();
            if crate::entry::is_concealed(&types) {
                continue;
            }
            let Some(captured) = read_clipboard() else {
                continue;
            };
            match store.record(captured) {
                Ok(Some(_)) => on_change(),
                Ok(None) => {}
                // Not the error's text: it can quote what was copied.
                Err(_) => tracing::warn!("could not store a clipboard entry"),
            }
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn spawn(
    _store: Arc<Store>,
    _active: impl Fn() -> bool + Send + 'static,
    _on_change: impl Fn() + Send + 'static,
) {
    // ponytail: no watcher off macOS. Windows has GetClipboardSequenceNumber,
    // the same cheap counter, and the same loop would sit on top of it.
}
