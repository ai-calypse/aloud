//! The recording pill: a small always-on-top window showing dictation state.
//!
//! Replaces the Swift app's borderless `NSPanel` + Metal shader. The shader
//! has no cross-platform equivalent, so the web layer approximates it with a
//! CSS backdrop blur — a visual downgrade, not a functional one.

use crate::pipeline::Phase;
use aloud_store::settings::{OverlayDisplay, OverlayStyle};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;
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

/// Tells the overlay to play its exit, ahead of the window actually going away.
///
/// The window is what makes the pill visible, so hiding it is a hard cut — the
/// panel cannot animate itself out of existence after the surface it is drawn
/// on has gone. This event is the gap: the pill collapses back into the notch
/// while the window is still up, and [`hide`] takes the window away once it has.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "overlay-dismiss")]
pub struct OverlayDismiss;

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
/// 40pt wide and the message would be invisible) they fall back to the drop
/// pill, exactly as `RecordingOverlay.swift` guards on `screenHasNotch` and
/// `forceDropDownPill`.
///
/// The fallback used to be the glass capsule. It is the drop pill now that the
/// capsule is gone — which is the better answer anyway: both weld to the top
/// edge, so a wings user who hits one of these cases sees the same surface move
/// rather than a different one appear somewhere else.
fn style_for(window: &WebviewWindow, phase: Phase) -> (OverlayStyle, Option<crate::notch::Notch>) {
    let notch = target_notch(window);
    match settings(window.app_handle()).overlay_style {
        OverlayStyle::DropPill => (OverlayStyle::DropPill, notch),
        OverlayStyle::Wings => match (matches!(phase, Phase::Error), notch) {
            (false, Some(notch)) => (OverlayStyle::Wings, Some(notch)),
            _ => (OverlayStyle::DropPill, notch),
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

    // Cancels a pending exit before it can hide the window out from under the
    // dictation that is starting now.
    HIDE_GENERATION.fetch_add(1, Ordering::SeqCst);
    // A dictation started over the top of an answer takes the window back: the
    // pill is click-through and drives its own dismissal again from here.
    PANEL_OPEN.store(false, Ordering::SeqCst);

    let (style, notch) = style_for(&window, phase);
    // Every remaining style welds to the screen's top edge, so the anchor no
    // longer depends on which one was picked. It did while the glass capsule
    // floated below the menu bar.
    if let Err(e) = position_top_center(&window, 0.0) {
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

/// How long the pill gets to collapse before the window is taken away.
///
/// Slightly shorter than the close spring's full settle: the last frames of a
/// spring move sub-pixel distances, and waiting for them only delays the window
/// teardown by time the user cannot see. Long enough that the collapse reads;
/// short enough that a dictation ending does not feel like it hangs.
/// One hold for both a 40pt status pill and a 330pt answer panel, which is not
/// the compromise it looks like: a spring's time to reach its target is set by
/// its response, not by how far it has to travel, so both land on the cutout at
/// the same moment. Measured at ~250ms for each (the sampling recipe in the
/// aloud-motion skill), with a little margin here.
///
/// There is an upper bound too, and it is closer than it used to be. The close
/// spring rings past the cutout and back out to ~44pt around 600ms; the drawn
/// size is floored so the undershoot never shows, but the return *would* — the
/// panel would visibly re-inflate. The window has to be gone well before that.
const EXIT_MS: u64 = 280;

/// Distinguishes "the hide I scheduled" from "a hide that has been overtaken".
///
/// Both [`show`] and [`hide`] bump this. A hide captures the value it produced
/// and, when its timer fires, only acts if nothing has happened since. Without
/// it, starting a dictation inside the exit window would show the pill and then
/// have the previous run's timer hide it again ~260ms later — a live recording
/// with no overlay, which is exactly the failure the user cannot diagnose.
static HIDE_GENERATION: AtomicU64 = AtomicU64::new(0);

/// True while the Ask panel owns the overlay window.
///
/// Every dictation ends by emitting a phase, and both Ask paths end on
/// `Phase::Idle` — which hides the overlay. That is right when the overlay is a
/// status pill and wrong now that the answer is drawn *in* it: the panel is
/// read, scrolled and dismissed by the user, not by the run that produced it.
/// So the run's hide is suppressed while this is set, and only [`end_answer`]
/// (Escape, the close button) or a new [`show`] takes the window away.
static PANEL_OPEN: AtomicBool = AtomicBool::new(false);

/// Hands the overlay to Ask or Agent, in place.
///
/// The drop-down panel is already on screen and already grown — it has been
/// saying "Transcribing…" — so this only changes what the panel is allowed to
/// do. The content itself arrives as events and the panel springs to fit it,
/// which is the whole point of answering here instead of in a second window
/// that would appear over the top of this one.
pub fn show_panel(app: &AppHandle) {
    let Some(window) = window(app) else {
        tracing::warn!("overlay window missing");
        return;
    };

    HIDE_GENERATION.fetch_add(1, Ordering::SeqCst);
    PANEL_OPEN.store(true, Ordering::SeqCst);

    let notch = target_notch(&window);
    // Forced rather than read from settings: the content is drawn inside the
    // drop-down panel's silhouette, and a 40pt wing has nowhere to put it. The
    // frontend makes the same call for the live transcript — see
    // `effectiveStyle` in overlay.tsx — so a wings user sees the same notch
    // pull down for a dictation, a question and an agent run alike.
    let style = OverlayStyleChanged {
        style: OverlayStyle::DropPill,
        notch_width: notch.map_or(0.0, |n| n.width),
        notch_height: notch.map_or(0.0, |n| n.height),
    };
    // Only moves anything if the run was drawing the glass pill, which hangs
    // below the menu bar rather than welding to it.
    if let Err(e) = position_top_center(&window, 0.0) {
        tracing::warn!(%e, "could not position answer panel");
    }
    let _ = window.show();
    if let Err(e) = style.emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send overlay style");
    }
    // Keys but not clicks, yet. The panel is typed into and dismissed with
    // Escape, and `ignoresMouseEvents` does not gate the keyboard, so it can
    // take focus while still letting every click through to whatever is
    // underneath. Clicks are handed to it only while the pointer is actually
    // over it — see [`track_cursor`].
    let _ = window.set_ignore_cursor_events(true);
    let _ = window.set_focus();
    track_cursor(app);
}

/// How often the pointer is checked against the panel while an answer is up.
///
/// The pointer has to be somewhere for a frame or two before a click can
/// follow it, so this is imperceptible in the direction that matters. Only
/// runs while an answer is on screen.
const CURSOR_POLL: Duration = Duration::from_millis(60);

/// The settled size of the drop-down panel, in logical pixels, reported by the
/// panel itself. Rounded — this decides which side of an edge the pointer is
/// on, not layout.
static PANEL_W: AtomicU32 = AtomicU32::new(0);
static PANEL_H: AtomicU32 = AtomicU32::new(0);

/// Records the panel's settled box, so the click region can be exactly it.
#[tauri::command]
#[specta::specta]
pub fn set_panel_box(width: f64, height: f64) {
    PANEL_W.store(width.max(0.0) as u32, Ordering::Relaxed);
    PANEL_H.store(height.max(0.0) as u32, Ordering::Relaxed);
}

/// Gives the window clicks only while the pointer is over the answer panel.
///
/// The overlay's canvas is deliberately far larger than anything drawn in it
/// (see overlay.css), which is free while it is click-through and ruinous the
/// moment it is not: a 1000x720 rectangle over the top of the screen that
/// swallows every click and keystroke aimed at the app underneath. macOS has
/// no per-region hit testing — `ignoresMouseEvents` is the whole window — and
/// a window that is ignoring the mouse cannot see it move, so the pointer
/// cannot be tracked from inside the webview either. Polling it from out here
/// is the way round that, and it is why this is a loop rather than an event.
///
/// ponytail: a poll. `NSTrackingArea` on the panel's own rect would be
/// event-driven, at the cost of maintaining an AppKit view that shadows a box
/// the web layer already owns.
fn track_cursor(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Matches what `show_answer` just set, so the first poll only acts if
        // the pointer really is over the panel.
        let mut clickable = false;
        loop {
            tokio::time::sleep(CURSOR_POLL).await;
            if !PANEL_OPEN.load(Ordering::SeqCst) {
                return;
            }
            let Some(window) = window(&app) else { return };
            let over = cursor_over_panel(&window).unwrap_or(false);
            if over != clickable {
                clickable = over;
                let _ = window.set_ignore_cursor_events(!over);
            }
        }
    });
}

/// Whether the pointer is inside the panel's box. Just the OS reads; the
/// decision itself is [`point_in_panel`].
fn cursor_over_panel(window: &WebviewWindow) -> tauri::Result<bool> {
    let panel = (
        f64::from(PANEL_W.load(Ordering::Relaxed)),
        f64::from(PANEL_H.load(Ordering::Relaxed)),
    );
    let scale = window.scale_factor()?;
    let origin = window.outer_position()?.to_logical::<f64>(scale);
    let canvas = window.outer_size()?.to_logical::<f64>(scale);
    let cursor = window.cursor_position()?.to_logical::<f64>(scale);

    Ok(point_in_panel(
        (cursor.x, cursor.y),
        (origin.x, origin.y),
        canvas.width,
        panel,
    ))
}

/// Whether `cursor` is over the panel drawn in a window at `origin`.
///
/// The panel is welded to the top of its window and centred across it, so its
/// box follows from the window's frame and the size the panel reported. Pure,
/// because this is the whole of the decision and every input is a number the
/// OS hands over — staging it live would mean parking a pointer by hand.
fn point_in_panel(
    cursor: (f64, f64),
    origin: (f64, f64),
    canvas_width: f64,
    panel: (f64, f64),
) -> bool {
    let (width, height) = panel;
    // Nothing reported yet: stay click-through. Guessing a box here would put
    // the dead zone back, which is the bug this exists to prevent.
    if width <= 0.0 || height <= 0.0 {
        return false;
    }
    let left = origin.0 + (canvas_width - width) / 2.0;
    cursor.0 >= left
        && cursor.0 <= left + width
        && cursor.1 >= origin.1
        && cursor.1 <= origin.1 + height
}

/// Dismisses the panel, letting it collapse back into the notch first.
pub fn end_panel(app: &AppHandle) {
    if !PANEL_OPEN.swap(false, Ordering::SeqCst) {
        return;
    }
    if let Some(window) = window(app) {
        // Click-through again immediately rather than after the collapse: the
        // window lingers for EXIT_MS, and a transparent canvas swallowing
        // clicks for a quarter second is worse than anything the last frames
        // of the animation buy.
        let _ = window.set_ignore_cursor_events(true);
    }
    hide(app);
}

pub fn hide(app: &AppHandle) {
    if window(app).is_none() {
        return;
    }
    // The run that produced the panel is over, but the panel is not.
    if PANEL_OPEN.load(Ordering::SeqCst) {
        return;
    }

    let generation = HIDE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    // If nothing is listening the event is dropped and the window still goes
    // away on the timer, so a failure here costs the animation, never the hide.
    if let Err(e) = OverlayDismiss.emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send overlay dismiss");
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(EXIT_MS)).await;

        // A show() landed while we were waiting: the pill on screen belongs to
        // a newer dictation and is not ours to hide.
        if HIDE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }

        if let Some(window) = window(&app) {
            let _ = window.hide();
        }
    });
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
/// Done entirely in physical pixels, and that is the point.
///
/// Every input here is already physical — `Monitor::size`, `Monitor::position`
/// and `Window::outer_size` all are — so converting them to logical first only
/// creates a way to be wrong. It used to divide all three by the *monitor's*
/// scale factor, including the window size, which is reported against whatever
/// screen the window is currently on. Any disagreement between those two scales
/// centred the window against a width it does not have; at 2x against 1x that
/// is half a window's width of error, which is the difference between the wings
/// sitting on the notch and sitting a third of the way across the menu bar.
///
/// Only the caller's margin is logical, because it is a design measurement.
pub fn position_top_center(window: &WebviewWindow, top_margin: f64) -> tauri::Result<()> {
    let Some(monitor) = resolve_monitor(window)? else {
        return Ok(()); // headless; leave it wherever it is
    };

    let screen = monitor.size();
    let origin = monitor.position();
    let size = window.outer_size()?;
    let x = origin.x + (screen.width as i32 - size.width as i32) / 2;
    let y = origin.y + (top_margin * monitor.scale_factor()) as i32;

    // The one input that is not a constant of the display, and the only thing
    // that can move a window nobody resized. Logged so that a misplaced overlay
    // is a line to read rather than a thing to reproduce.
    tracing::debug!(
        screen_w = screen.width,
        window_w = size.width,
        scale = monitor.scale_factor(),
        x,
        y,
        "positioning overlay"
    );

    window.set_position(tauri::PhysicalPosition::new(x, y))?;
    Ok(())
}

/// Sends the live input level to the pill.
pub fn send_level(app: &AppHandle, level: f32) {
    let _ = AudioLevel(level).emit_to(app, LABEL);
}

/// Sends the previewed words so far to the pill, which grows to fit them.
///
/// This used to go to a second window that dropped below the pill. It goes to
/// the pill itself now: the words are what the user is watching, and a panel
/// welded to the notch can grow to hold them, which is the thing a fixed-width
/// status line never could.
///
/// An empty string is how a caller says "there is no preview any more" — the
/// pill falls back to its status word.
pub fn send_partial(app: &AppHandle, text: &str) {
    let _ = DictationPartial(text.to_string()).emit_to(app, LABEL);
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

    /// The reported bug: with an answer open, most of the screen stopped
    /// taking clicks.
    ///
    /// The overlay's canvas is far larger than the panel drawn in it, and
    /// handing the window the mouse handed it the whole canvas — a 1000x720
    /// rectangle across the top of the screen that swallowed every click and
    /// keystroke meant for the app underneath. Everything outside the panel's
    /// own box has to read as "not over the panel".
    #[test]
    fn only_the_panel_itself_takes_clicks() {
        // A 512x332 panel centred in the 1000x720 canvas, window at the top
        // left of a screen — i.e. what a real answer measures.
        let origin = (200.0, 0.0);
        let canvas = 1000.0;
        let panel = (512.0, 332.0);
        let over = |x, y| point_in_panel((x, y), origin, canvas, panel);

        // The panel spans x 444..956, y 0..332.
        assert!(over(700.0, 160.0), "the middle of the panel takes clicks");
        assert!(over(444.0, 0.0), "and so does its top-left corner");

        // The three regions the bug covered. Below is the big one: the canvas
        // ran 388pt further down the screen than the panel ever did.
        assert!(!over(700.0, 500.0), "below the panel must click through");
        assert!(!over(300.0, 160.0), "left of the panel must click through");
        assert!(!over(950.0, 400.0), "the canvas corner must click through");

        // Before the panel has reported a size there is no box to be inside,
        // and the fallback has to be click-through rather than the old
        // whole-canvas behaviour.
        assert!(!point_in_panel((700.0, 160.0), origin, canvas, (0.0, 0.0)));
    }
}
