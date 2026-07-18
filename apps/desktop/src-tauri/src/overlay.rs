//! The recording pill: a small always-on-top window showing dictation state.
//!
//! Replaces the Swift app's borderless `NSPanel` + Metal shader. The shader
//! has no cross-platform equivalent, so the web layer approximates it with a
//! CSS backdrop blur — a visual downgrade, not a functional one.

use crate::pipeline::Phase;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_specta::Event;

const LABEL: &str = "overlay";
/// Distance from the top of the screen, in logical pixels. Clears the menu bar.
const TOP_MARGIN: f64 = 48.0;

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// Shows the pill for `phase`, positioned top-centre of the focused monitor.
///
/// The Swift app tucked this into the notch; there is no cross-platform way to
/// find one, and Windows has no notch at all.
/// ponytail: fixed top-centre placement — notch awareness is a macOS-only
/// enhancement if it is ever missed.
pub fn show(app: &AppHandle, phase: Phase) {
    let Some(window) = window(app) else {
        tracing::warn!("overlay window missing");
        return;
    };

    if let Err(e) = position_top_center(&window, TOP_MARGIN) {
        tracing::warn!(%e, "could not position overlay");
    }

    // Click-through: the pill must never steal focus or clicks from the app
    // the user is dictating into.
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.show();

    // Re-send the phase to this window specifically. A hidden window's webview
    // is created lazily, so it can miss the app-wide event that triggered the
    // show and stay stuck rendering its initial idle state — invisible.
    if let Err(e) = phase.emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send phase to overlay");
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = window(app) {
        let _ = window.hide();
    }
}

/// Top-centres `window` on whichever monitor holds the cursor. Shared with
/// the ask/agent panels, which drop from the top the way the Swift app's
/// answer panel does.
pub fn position_top_center(window: &WebviewWindow, top_margin: f64) -> tauri::Result<()> {
    // The cursor's monitor, not the primary one: on a multi-monitor desk the
    // user is looking at the screen they are working on.
    let monitor = match window.cursor_position() {
        Ok(cursor) => window.monitor_from_point(cursor.x, cursor.y)?,
        Err(_) => window.primary_monitor()?,
    };
    let Some(monitor) = monitor.or(window.primary_monitor()?) else {
        return Ok(()); // headless; leave it wherever it is
    };

    let scale = monitor.scale_factor();
    let screen = monitor.size().to_logical::<f64>(scale);
    let origin = monitor.position().to_logical::<f64>(scale);
    let size = window.outer_size()?.to_logical::<f64>(scale);

    window.set_position(tauri::LogicalPosition::new(
        origin.x + (screen.width - size.width) / 2.0,
        origin.y + top_margin,
    ))?;
    Ok(())
}
