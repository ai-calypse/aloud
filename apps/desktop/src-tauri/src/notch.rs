//! Measuring the MacBook notch, so the wings overlay welds to it.
//!
//! `NSScreen` is AppKit, so [`notched_screens`] must run on the main thread —
//! it is called once from `setup`. No notch means no result, and the overlay
//! falls back to a pill (see `overlay.rs`).
//!
//! ponytail: measured once at launch. A display attached later, or a lid shut
//! over an external, is not re-measured until relaunch — an
//! `NSApplication.didChangeScreenParameters` observer is the upgrade path.

/// One notched screen's geometry, in logical points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Notch {
    /// Point size is how a Tauri monitor is matched back to this screen —
    /// Tauri and AppKit disagree on origin and axis, but not on size.
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
        // a crash.
        return Vec::new();
    };

    let mut out = Vec::new();
    let screens = NSScreen::screens(mtm);
    for i in 0..screens.count() {
        let screen = unsafe { screens.objectAtIndex(i) };
        // safeAreaInsets.top is the notch height, and 0 without one — the
        // cheapest "has a notch?" test AppKit offers.
        let top = unsafe { screen.safeAreaInsets() }.top;
        if top <= 0.0 {
            continue;
        }
        let frame = screen.frame();
        // The notch is the strip the two menu-bar auxiliary areas leave open.
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
