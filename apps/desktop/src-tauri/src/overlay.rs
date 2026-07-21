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

/// The words so far, while the live preview is on.
///
/// **Feedback only — none of this is ever pasted.** It is a rolling re-read of
/// the last few seconds by the small local model, so it revises itself as a
/// sentence lands. The panel is the one surface where a revision costs nothing,
/// so it is the only one that sees this; the paste is a separate, single pass
/// over the whole recording once the key is up.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "dictation-partial")]
pub struct DictationPartial(pub String);

/// Why a dictation failed, in words the user can act on. `Phase::Error` says
/// only that something broke, which the pill used to render as "Failed".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "overlay-error")]
pub struct OverlayError(pub String);

/// The style the overlay window should draw, pushed on every show.
///
/// Sent rather than read from settings by the webview: the overlay has no
/// settings binding of its own, and a style change has to reach a window that
/// is already mounted. Carries the notch cutout so the wings can weld to it —
/// zero when not welding (any pill, or a screen with no notch).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "overlay-style")]
pub struct OverlayStyleChanged {
    pub style: OverlayStyle,
    /// Logical-pixel width of the notch gap between the two wings.
    pub notch_width: f64,
    /// Logical-pixel height of the wings, matching the notch's safe-area inset.
    pub notch_height: f64,
}

const LABEL: &str = "overlay";
/// The streaming-transcript panel, shown below the pill while recording with
/// streaming transcription on. Separate window so the pill stays small.
const TRANSCRIPT_LABEL: &str = "transcript";
/// Distance from the top of the screen, in logical pixels. Clears the menu bar.
const TOP_MARGIN: f64 = 48.0;
/// The transcript panel drops below the pill; the pill is ~52pt tall at
/// `TOP_MARGIN`, so this clears it with a little air.
const TRANSCRIPT_TOP_MARGIN: f64 = 116.0;

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// Lifts the overlay above the menu bar so the wings can draw *into* the notch.
///
/// Tauri's `always_on_top` sits a window at the floating level (3), which is
/// below the menu bar (24). macOS then refuses to let the window cover the
/// menu-bar strip and clamps it down — which is why the wings landed low,
/// roughly where the pill sits. voice-dict's panel uses `.screenSaver` (1000)
/// for exactly this reason; matching it lets the top-anchored window keep its
/// `y = 0` and paint over the notch.
///
/// Must run on the main thread — AppKit. Called from `setup`, which is on it.
#[cfg(target_os = "macos")]
pub fn raise_over_menu_bar(app: &AppHandle) {
    use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindow};

    let Some(window) = window(app) else { return };
    let Ok(ptr) = window.ns_window() else { return };
    // Safety: `ns_window` returns this webview's live `NSWindow`.
    let ns_window: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
    ns_window.setLevel(NSScreenSaverWindowLevel);
}

#[cfg(not(target_os = "macos"))]
pub fn raise_over_menu_bar(_app: &AppHandle) {}

/// The notch of the screen this overlay will land on, if that screen has one.
///
/// Matches the target monitor to a measured notch by logical size — Tauri and
/// AppKit disagree on origin and axis, but a screen's point size is the same in
/// both. Shares `resolve_monitor` with positioning, so the two agree on which
/// screen the overlay is on.
fn target_notch(window: &WebviewWindow) -> Option<crate::notch::Notch> {
    let notches = window
        .app_handle()
        .try_state::<crate::AppState>()
        .map(|s| s.notch.clone())?;
    if notches.is_empty() {
        return None;
    }

    let monitor = resolve_monitor(window).ok().flatten()?;
    let size = monitor.size().to_logical::<f64>(monitor.scale_factor());
    notches
        .into_iter()
        .find(|n| (n.screen_w - size.width).abs() < 2.0 && (n.screen_h - size.height).abs() < 2.0)
}

/// The style to draw for `phase`, and the notch geometry the top-welded styles
/// need.
///
/// The glass pill floats and needs nothing. The drop-down pill welds to the top
/// and pads its content below the notch, so it takes the measurement but works
/// on any screen. Wings *flank* the notch, so they only make sense on a notched
/// macOS screen: off macOS, on an external display, or on an error (a wing is
/// 40pt wide and the message would be invisible) they collapse to the glass
/// pill, exactly as `RecordingOverlay.swift` guards on `screenHasNotch` and
/// `forceDropDownPill`.
fn style_for(window: &WebviewWindow, phase: Phase) -> (OverlayStyle, Option<crate::notch::Notch>) {
    match settings(window.app_handle()).overlay_style {
        OverlayStyle::LiquidGlass => (OverlayStyle::LiquidGlass, None),
        OverlayStyle::Pill => (OverlayStyle::Pill, target_notch(window)),
        OverlayStyle::Wings => match (matches!(phase, Phase::Error), target_notch(window)) {
            (false, Some(notch)) => (OverlayStyle::Wings, Some(notch)),
            _ => (OverlayStyle::LiquidGlass, None),
        },
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

    let (style, notch) = style_for(&window, phase);
    // Wings weld to the top edge; both pills hang below the menu bar. Same
    // window, same positioning — only the anchor changes.
    let margin = match style {
        // Both top-welded styles anchor at the screen edge; only the floating
        // glass pill drops below the menu bar.
        OverlayStyle::Wings | OverlayStyle::Pill => 0.0,
        OverlayStyle::LiquidGlass => TOP_MARGIN,
    };
    if let Err(e) = position_top_center(&window, margin) {
        tracing::warn!(%e, "could not position overlay");
    }
    // Click-through: the pill must never steal focus or clicks from the app
    // the user is dictating into.
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.show();

    // Both sent *after* `show`, for the reason the phase re-send already
    // documented: a hidden window's webview is created lazily, so an event
    // emitted before the show can land before anything is listening. The style
    // event was emitted above the show and is exactly the one that would be
    // dropped — leaving the overlay drawing a pill on first appearance despite
    // wings being configured.
    let style_event = OverlayStyleChanged {
        style,
        notch_width: notch.map_or(0.0, |n| n.width),
        notch_height: notch.map_or(0.0, |n| n.height),
    };
    if let Err(e) = style_event.emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send overlay style");
    }
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

/// Which monitor index to place the overlay on, given what is plugged in.
///
/// Pure, so the case that cannot be staged on one machine — the chosen display
/// unplugged between two dictations — is actually testable. `primary` is the
/// index the OS calls primary; `cursor` is the monitor holding the pointer.
///
/// Returning `None` means "leave the window where it is", which only happens
/// with no monitors at all.
fn chosen_monitor(
    display: &OverlayDisplay,
    names: &[Option<String>],
    primary: Option<usize>,
    cursor: Option<usize>,
) -> Option<usize> {
    let picked = match display {
        OverlayDisplay::Active => cursor,
        OverlayDisplay::Primary => primary,
        OverlayDisplay::Named(wanted) => named_monitor(names, wanted),
    };
    // The fallback that keeps the overlay on screen: a display that is gone,
    // or a cursor the OS will not locate, resolves to the primary rather than
    // to coordinates on a monitor that no longer exists.
    picked.or(primary)
}

/// The monitor the overlay should land on, per the "Show on" setting, falling
/// back to the primary. `Ok(None)` only with no monitors at all (headless).
///
/// Shared by positioning and notch lookup so the two never disagree about which
/// screen the overlay is on. The `chosen_monitor` decision it wraps is pure and
/// tested; this is just the OS calls that feed it.
fn resolve_monitor(window: &WebviewWindow) -> tauri::Result<Option<tauri::Monitor>> {
    let mut monitors = window.available_monitors()?;
    let names: Vec<_> = monitors.iter().map(|m| m.name().cloned()).collect();
    let index_of = |target: Option<tauri::Monitor>| {
        // Compared by position: `Monitor` is not `PartialEq`, and two monitors
        // cannot share an origin.
        target.and_then(|t| monitors.iter().position(|m| m.position() == t.position()))
    };
    let primary = index_of(window.primary_monitor()?);
    let cursor = match window.cursor_position() {
        Ok(at) => index_of(window.monitor_from_point(at.x, at.y)?),
        Err(_) => None,
    };
    let display = settings(window.app_handle()).overlay_display;
    Ok(chosen_monitor(&display, &names, primary, cursor).map(|i| monitors.swap_remove(i)))
}

/// Top-centres `window` on the monitor the user picked. Shared with the
/// ask/agent panels, which drop from the top the way the Swift app's answer
/// panel does — and follow the same display, so the three never split up.
pub fn position_top_center(window: &WebviewWindow, top_margin: f64) -> tauri::Result<()> {
    let Some(monitor) = resolve_monitor(window)? else {
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

/// Sends the previewed words so far to the live-transcript panel.
///
/// The pill deliberately no longer shows this — a clipped 60-char line was a
/// poor place to read a growing transcript, and the panel is the surface for it.
pub fn send_partial(app: &AppHandle, text: &str) {
    let _ = DictationPartial(text.to_string()).emit_to(app, TRANSCRIPT_LABEL);
}

/// Shows the streaming-transcript panel below the pill.
///
/// Click-through and never focused, so it cannot steal keystrokes from the app
/// being dictated into. Called only where a preview has just been armed to fill
/// it — the caller owns that decision, because it is the one that knows whether
/// this capture is a dictation at all.
pub fn show_transcript(app: &AppHandle) {
    let Some(window) = app.get_webview_window(TRANSCRIPT_LABEL) else {
        return;
    };
    if let Err(e) = position_top_center(&window, TRANSCRIPT_TOP_MARGIN) {
        tracing::warn!(%e, "could not position transcript panel");
    }
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.show();
    // Clear the last run's text *after* the show: a hidden window's webview is
    // created lazily, so an event sent before it would land before anything is
    // listening and the panel would flash the previous dictation. The panel
    // treats an empty partial as "Listening…".
    let _ = DictationPartial(String::new()).emit_to(app, TRANSCRIPT_LABEL);
}

pub fn hide_transcript(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(TRANSCRIPT_LABEL) {
        let _ = window.hide();
    }
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

    /// The failure this prevents, and the one ROADMAP 7.2 names as *the* test:
    /// a display picked by name is unplugged, and the overlay is positioned
    /// against a monitor that is no longer there — i.e. off-screen.
    ///
    /// This exercises the fallback itself, not just the name lookup. An
    /// earlier version asserted only `named_monitor`, which is a one-line
    /// wrapper over `Iterator::position` — it proved the stdlib works and left
    /// the actual `.or(primary)` decision uncovered.
    #[test]
    fn unplugging_the_chosen_display_falls_back_to_primary() {
        let built_in = Some("Built-in Retina Display".to_string());
        let dell = Some("DELL U2720Q".to_string());

        let plugged = [built_in.clone(), dell];
        let named = OverlayDisplay::Named("DELL U2720Q".into());
        assert_eq!(chosen_monitor(&named, &plugged, Some(0), Some(0)), Some(1));

        // Unplugged: the chosen name resolves to nothing and must land on the
        // primary rather than on the index it used to occupy.
        let unplugged = [built_in];
        assert_eq!(
            chosen_monitor(&named, &unplugged, Some(0), Some(0)),
            Some(0),
            "a display that is gone must fall back, not render off-screen"
        );

        // A monitor the OS gives no name to must never win a lookup by name.
        assert_eq!(chosen_monitor(&named, &[None], Some(0), None), Some(0));

        // The other two modes, including a cursor the OS will not locate.
        assert_eq!(
            chosen_monitor(&OverlayDisplay::Active, &unplugged, Some(0), Some(1)),
            Some(1)
        );
        assert_eq!(
            chosen_monitor(&OverlayDisplay::Active, &unplugged, Some(0), None),
            Some(0),
            "no cursor monitor still has to put the overlay somewhere"
        );
        assert_eq!(
            chosen_monitor(&OverlayDisplay::Primary, &unplugged, Some(0), Some(1)),
            Some(0)
        );

        // Headless: nothing to position against, and no panic.
        assert_eq!(chosen_monitor(&named, &[], None, None), None);
    }
}
