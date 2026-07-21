//! Measuring the MacBook notch, so the wings overlay welds to it.
//!
//! voice-dict flanks the physical camera cutout with two menu-bar tabs; the
//! blend only reads as native when the gap between the tabs is exactly the
//! notch width and their height is exactly the safe-area inset. Both come from
//! `NSScreen` (AppKit), so [`notched_screens`] must run on the main thread —
//! it is called once from `setup`, which does. Non-macOS and non-notched Macs
//! return nothing, and the overlay falls back to a pill (see `overlay.rs`).
//!
//! ponytail: measured once at launch. A display attached later, or a lid shut
//! over an external, is not re-measured until relaunch — an
//! `NSApplication.didChangeScreenParameters` observer is the upgrade path.

/// One notched screen's geometry, in logical points.
///
/// Plain data — no AppKit type — so it can live in the cross-platform
/// `AppState` and be compared against a Tauri monitor without a feature gate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Notch {
    /// The screen's own logical size, which is how a Tauri monitor is matched
    /// back to it: Tauri and AppKit disagree on origin and axis, but a screen's
    /// point size is the same in both.
    pub screen_w: f64,
    pub screen_h: f64,
    /// The cutout to leave clear: the gap between the wings, and their height.
    pub width: f64,
    pub height: f64,
}

#[cfg(target_os = "macos")]
pub fn notched_screens() -> Vec<Notch> {
    use objc2_app_kit::NSScreen;
    use objc2_foundation::MainThreadMarker;

    let Some(mtm) = MainThreadMarker::new() else {
        // Off the main thread AppKit is not ours to touch; better no notch than
        // a crash. `setup` calls this on the main thread, so this is defensive.
        return Vec::new();
    };

    let mut out = Vec::new();
    let screens = NSScreen::screens(mtm);
    for i in 0..screens.count() {
        let screen = unsafe { screens.objectAtIndex(i) };
        // safeAreaInsets.top is the notch height, and 0 on any screen without
        // one — the cheapest "has a notch?" test AppKit offers.
        let top = unsafe { screen.safeAreaInsets() }.top;
        if top <= 0.0 {
            continue;
        }
        let frame = screen.frame();
        // The notch is the middle strip the two menu-bar auxiliary areas leave
        // open between them.
        let left = unsafe { screen.auxiliaryTopLeftArea() };
        let right = unsafe { screen.auxiliaryTopRightArea() };
        let width = frame.size.width - left.size.width - right.size.width;
        out.push(Notch {
            screen_w: frame.size.width,
            screen_h: frame.size.height,
            width: width.max(0.0),
            height: top,
        });
    }
    out
}

#[cfg(not(target_os = "macos"))]
pub fn notched_screens() -> Vec<Notch> {
    Vec::new()
}
