//! Dragging a real file out of the notch into another app.
//!
//! Tauri has no drag-OUT API — a system drag must originate from an `NSView`
//! with the live mousedown `NSEvent`, which a JS click cannot supply. So the
//! frontend *arms* a path on `mousedown` ([`arm`]), and a local mouse-event
//! monitor installed over the overlay view ([`install`]) catches the following
//! `mouseDragged`, reads the armed path, and starts an `NSDraggingSession` with
//! a file-URL pasteboard item — which Finder, Slack, and Mail accept as a real
//! file.
//!
//! Everything here is main-thread AppKit, like the notch code in `overlay.rs`.
//! Off macOS the whole crate is inert.

#[cfg(target_os = "macos")]
mod imp;

use std::sync::Mutex;

/// The path the frontend last armed for dragging, consumed by the next drag the
/// monitor sees. `None` when nothing is armed, so a stray drag on empty panel
/// chrome starts nothing.
static ARMED: Mutex<Option<String>> = Mutex::new(None);

/// Stash a path to drag on the next mouse-drag. Called from the frontend's
/// `mousedown` on a draggable result, before the drag gesture begins. Cheap and
/// thread-safe; the actual drag starts on the main thread in the monitor.
pub fn arm(path: impl Into<String>) {
    let path = path.into();
    tracing::debug!(%path, "armed file drag");
    *ARMED.lock().unwrap() = Some(path);
}

/// Clear any armed path — e.g. on mouseup without a drag, so a later plain click
/// never turns into a drag.
pub fn disarm() {
    *ARMED.lock().unwrap() = None;
}

/// Takes the armed path, leaving none. The monitor calls this the instant a drag
/// begins, so two drags never share one arm.
fn take() -> Option<String> {
    ARMED.lock().unwrap().take()
}

/// Installs the drag machinery on the overlay's `NSView`. Must run on the main
/// thread. `ns_view` is the webview's content view pointer (`window.ns_view()`
/// in Tauri). No-op if called twice or off macOS.
///
/// # Safety
/// `ns_view` must be a live `NSView*` for the overlay window, valid for the
/// lifetime of the app (it is — the overlay window is never destroyed).
pub unsafe fn install(ns_view: *mut std::ffi::c_void) {
    #[cfg(target_os = "macos")]
    {
        imp::install(ns_view);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ns_view;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_take_is_one_shot_and_disarm_clears() {
        // Serialized here since ARMED is process-global; these run in one test
        // to avoid cross-test races on the static.
        arm("/tmp/a.png");
        assert_eq!(take().as_deref(), Some("/tmp/a.png"));
        // Taken once — a second take is empty, so two drags never share an arm.
        assert_eq!(take(), None);

        // Disarm (the mouseup path) clears a pending arm, so a click that armed
        // a path cannot drag it on a later gesture.
        arm("/tmp/b.png");
        disarm();
        assert_eq!(take(), None);
    }
}
