//! The recording pill: a small always-on-top window showing dictation state.
//!
//! Replaces the Swift app's borderless `NSPanel` + Metal shader. The shader
//! has no cross-platform equivalent, so the web layer approximates it with a
//! CSS backdrop blur — a visual downgrade, not a functional one.

use crate::pipeline::Phase;
use aloud_store::settings::{OverlayDisplay, OverlayStyle};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_specta::Event;

/// Live input level, 0.0–1.0, driving the pill's waveform. Sent only to the
/// overlay window and only while recording; the bars were previously a fixed
/// CSS animation that looked identical in silence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "audio-level")]
pub struct AudioLevel(pub f32);

/// Model download progress, 0.0–1.0. First use of the local engine fetches
/// ~150MB; without this the pill sits on "Transcribing…" for minutes and the
/// app looks hung.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "model-download")]
pub struct ModelDownload(pub f32);

/// Why a dictation failed, in words the user can act on. `Phase::Error` says
/// only that something broke, which the pill used to render as "Failed".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "overlay-error")]
pub struct OverlayError(pub String);

/// The style the overlay window should draw, pushed on every show.
///
/// Sent rather than read from settings by the webview: the overlay has no
/// settings binding of its own, and a style change has to reach a window that
/// is already mounted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "overlay-style")]
pub struct OverlayStyleChanged(pub OverlayStyle);

const LABEL: &str = "overlay";
/// Distance from the top of the screen, in logical pixels. Clears the menu bar.
const TOP_MARGIN: f64 = 48.0;

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// The style the overlay should draw for `phase`.
///
/// Wings are macOS-only, and an error never gets them: a wing is 40pt wide and
/// the message would be invisible, so a failure the user has to read forces
/// the pill back, exactly as `RecordingOverlay.swift` does with
/// `forceDropDownPill`.
fn style_for(app: &AppHandle, phase: Phase) -> OverlayStyle {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, phase);
        OverlayStyle::Pill
    }
    #[cfg(target_os = "macos")]
    {
        match settings(app).overlay_style {
            OverlayStyle::Wings if !matches!(phase, Phase::Error) => OverlayStyle::Wings,
            _ => OverlayStyle::Pill,
        }
    }
}

fn settings(app: &AppHandle) -> aloud_store::Settings {
    // Before `setup` finishes there is no state; the defaults are what the
    // window was built with anyway.
    app.try_state::<crate::AppState>()
        .and_then(|state| state.settings.lock().ok().map(|s| s.clone()))
        .unwrap_or_default()
}

/// Shows the overlay for `phase`, positioned top-centre of the chosen monitor.
///
/// The Swift app tucked this into the notch; there is no cross-platform way to
/// find one, so the wings style flanks the middle of the menu bar by a fixed
/// gap instead.
/// ponytail: the notch is not measured — `NSScreen.auxiliaryTopLeftArea` is
/// the upgrade path, at the cost of an objc2-app-kit dependency.
pub fn show(app: &AppHandle, phase: Phase) {
    let Some(window) = window(app) else {
        tracing::warn!("overlay window missing");
        return;
    };

    let style = style_for(app, phase);
    // Wings weld to the top edge; the pill hangs below the menu bar. Same
    // window, same positioning — only the anchor changes.
    let margin = match style {
        OverlayStyle::Pill => TOP_MARGIN,
        OverlayStyle::Wings => 0.0,
    };
    if let Err(e) = position_top_center(&window, margin) {
        tracing::warn!(%e, "could not position overlay");
    }
    if let Err(e) = OverlayStyleChanged(style).emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send overlay style");
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

/// Index of the monitor called `wanted`, or None when nothing is called that.
///
/// Pulled out of [`position_top_center`] so the case that cannot be reproduced
/// on one machine — the chosen display unplugged between two dictations — is
/// testable. None is not a failure: the caller reads it as "fall back to the
/// primary", which is what keeps the overlay on screen.
fn named_monitor(names: &[Option<String>], wanted: &str) -> Option<usize> {
    names.iter().position(|n| n.as_deref() == Some(wanted))
}

/// Top-centres `window` on the monitor the user picked. Shared with the
/// ask/agent panels, which drop from the top the way the Swift app's answer
/// panel does — and follow the same display, so the three never split up.
pub fn position_top_center(window: &WebviewWindow, top_margin: f64) -> tauri::Result<()> {
    let monitor = match settings(window.app_handle()).overlay_display {
        // The cursor's monitor, not the primary one: on a multi-monitor desk
        // the user is looking at the screen they are working on.
        OverlayDisplay::Active => match window.cursor_position() {
            Ok(cursor) => window.monitor_from_point(cursor.x, cursor.y)?,
            Err(_) => window.primary_monitor()?,
        },
        OverlayDisplay::Primary => window.primary_monitor()?,
        OverlayDisplay::Named(wanted) => {
            let mut monitors = window.available_monitors()?;
            let names: Vec<_> = monitors.iter().map(|m| m.name().cloned()).collect();
            named_monitor(&names, &wanted).map(|i| monitors.swap_remove(i))
        }
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

/// Sends the live input level to the pill.
pub fn send_level(app: &AppHandle, level: f32) {
    let _ = AudioLevel(level).emit_to(app, LABEL);
}

/// Sends the message the pill shows instead of a bare "Failed".
pub fn send_error(app: &AppHandle, message: &str) {
    if let Err(e) = OverlayError(message.to_string()).emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send error to overlay");
    }
}

/// Sends model-download progress to the pill.
pub fn send_download(app: &AppHandle, fraction: f32) {
    let _ = ModelDownload(fraction).emit_to(app, LABEL);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The failure this prevents: a display picked by name is unplugged, the
    /// lookup finds nothing, and the overlay is positioned against a monitor
    /// that is no longer there — i.e. off-screen. None here is what makes the
    /// caller's `.or(primary_monitor())` fire.
    #[test]
    fn unplugging_the_chosen_display_falls_back_instead_of_matching() {
        let plugged = [
            Some("Built-in Retina Display".to_string()),
            Some("DELL U2720Q".to_string()),
        ];
        assert_eq!(named_monitor(&plugged, "DELL U2720Q"), Some(1));

        let unplugged = [Some("Built-in Retina Display".to_string())];
        assert_eq!(named_monitor(&unplugged, "DELL U2720Q"), None);

        // A monitor the OS gives no name to must never win a lookup by name.
        assert_eq!(named_monitor(&[None], "DELL U2720Q"), None);
    }
}
