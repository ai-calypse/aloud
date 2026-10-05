//! Displays in AppKit points, and where a top-centred window lands on one.
//!
//! Everything here is in AppKit's global space: points, origin at the bottom-left
//! of the primary display, y up. That is the one space every display shares
//! whatever its scale factor. Tao's physical pixels are not: it turns a
//! monitor's origin into pixels with *that monitor's* scale, and a window's
//! size and `set_position` with *the window's current* scale, so on a 1x
//! external beside the 2x built-in the two disagree by a factor of two and a
//! window centred "on" one screen lands across the seam of the next.
//!
//! The geometry is pure and runs anywhere; the `NSScreen` reads are macOS-only
//! and must run on the main thread ([`refresh`]).

#[cfg(target_os = "macos")]
mod apple;
#[cfg(target_os = "macos")]
pub use apple::{mouse_location, refresh, watch};

use std::sync::Mutex;

/// A rectangle in AppKit points: `(x, y)` is the bottom-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn max_x(&self) -> f64 {
        self.x + self.w
    }

    pub fn max_y(&self) -> f64 {
        self.y + self.h
    }

    /// Left and top edges are in, right and bottom are out. The top is the odd
    /// one: the cursor in a display's top row of pixels reports `y == max_y`
    /// exactly, and that row is the notch — the one place the pointer has to
    /// count as on this screen.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.max_x() && y > self.y && y <= self.max_y()
    }
}

/// The cutout to leave clear on a notched display, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Notch {
    /// The gap between the two wings, and their height.
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    /// `localizedName`: what System Settings › Displays calls it.
    pub name: String,
    pub frame: Rect,
    /// Backing scale factor, for the log only — nothing here depends on it.
    pub scale: f64,
    pub notch: Option<Notch>,
}

impl std::fmt::Display for Screen {
    /// One line for a log: enough to see which display is which and whether a
    /// frame left its screen.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let r = self.frame;
        write!(
            f,
            "\"{}\" x={} y={} {}x{} @{}x{}",
            self.name,
            r.x,
            r.y,
            r.w,
            r.h,
            self.scale,
            if self.notch.is_some() { " notch" } else { "" }
        )
    }
}

/// Every display in `NSScreen.screens` order, so index 0 is the menu-bar
/// screen. Written by [`refresh`].
static SCREENS: Mutex<Vec<Screen>> = Mutex::new(Vec::new());

/// The displays as of the last [`refresh`], readable from any thread. AppKit
/// itself is main-thread only, and the overlay positions from tokio workers.
pub fn current() -> Vec<Screen> {
    SCREENS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn store(screens: Vec<Screen>) {
    *SCREENS.lock().unwrap_or_else(|e| e.into_inner()) = screens;
}

/// The screen holding `point`, by index into `screens`.
pub fn locate(screens: &[Screen], (x, y): (f64, f64)) -> Option<usize> {
    screens.iter().position(|s| s.frame.contains(x, y))
}

/// Where a `window` (w, h) top-centred on `screen` goes, as the top-left corner
/// `NSWindow::setFrameTopLeftPoint` wants: the window's left edge, and its top
/// edge `top_margin` points below the screen's.
///
/// Only the chosen screen's frame matters — the others sit elsewhere in the
/// same space and cannot be overlapped by a frame that stays inside this one.
/// That is the point: the result is clamped to `screen`, so a window never
/// straddles a seam. One wider or taller than the screen cannot fit at all; it
/// pins to the screen's top-left corner and overhangs right and down.
pub fn top_left_for(screen: Rect, (w, h): (f64, f64), top_margin: f64) -> (f64, f64) {
    // Whole points: half a point is a blurred edge on a 1x display.
    let x = (screen.x + (screen.w - w) / 2.0).round();
    let top = (screen.max_y() - top_margin).round();
    (
        x.clamp(screen.x, screen.x + (screen.w - w).max(0.0)),
        top.clamp(screen.y + h.min(screen.h), screen.max_y()),
    )
}

#[cfg(not(target_os = "macos"))]
pub fn refresh() {}

#[cfg(not(target_os = "macos"))]
pub fn watch(_on_change: impl Fn() + Send + Sync + 'static) {}

#[cfg(not(target_os = "macos"))]
pub fn mouse_location() -> Option<(f64, f64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: (f64, f64) = (1000.0, 720.0);

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    /// 14" MacBook, 2x, at the origin — the primary.
    fn built_in() -> Rect {
        rect(0.0, 0.0, 1512.0, 982.0)
    }

    fn lands_inside(screen: Rect, window: (f64, f64), margin: f64) -> bool {
        let (x, top) = top_left_for(screen, window, margin);
        x >= screen.x
            && x + window.0 <= screen.max_x()
            && top <= screen.max_y()
            && top - window.1 >= screen.y
    }

    #[test]
    fn a_panel_is_centred_along_the_top_of_its_screen() {
        assert_eq!(top_left_for(built_in(), WINDOW, 0.0), (256.0, 982.0));
        assert_eq!(top_left_for(built_in(), WINDOW, 8.0), (256.0, 974.0));
    }

    /// The reported bug. A 1x 1920-wide external to the right starts at
    /// x = 1512; the panel must sit in 1512..3432, not straddle the seam.
    #[test]
    fn a_panel_never_straddles_two_screens() {
        let external = rect(1512.0, -98.0, 1920.0, 1080.0);
        let (x, top) = top_left_for(external, WINDOW, 0.0);
        assert_eq!((x, top), (1972.0, 982.0));
        assert!(x >= 1512.0 && x + 1000.0 <= 3432.0);
        assert!(lands_inside(external, WINDOW, 0.0));
    }

    #[test]
    fn an_external_to_the_left_has_negative_coordinates() {
        let external = rect(-1920.0, 0.0, 1920.0, 1080.0);
        let (x, top) = top_left_for(external, WINDOW, 0.0);
        assert_eq!((x, top), (-1460.0, 1080.0));
        assert!(lands_inside(external, WINDOW, 0.0));
    }

    #[test]
    fn an_external_above_sits_at_its_own_top_not_the_built_ins() {
        let external = rect(-204.0, 982.0, 1920.0, 1080.0);
        let (x, top) = top_left_for(external, WINDOW, 0.0);
        assert_eq!((x, top), (256.0, 2062.0));
        assert!(lands_inside(external, WINDOW, 0.0));
    }

    /// A 2x built-in beside a 1x external and a 3008-point 6K display: the same
    /// window in points fits each by its own width, whatever the backing scale.
    #[test]
    fn screens_of_different_scale_factors_each_get_their_own_centre() {
        for (screen, centre) in [
            (rect(0.0, 0.0, 1512.0, 982.0), 756.0),
            (rect(1512.0, 0.0, 1920.0, 1080.0), 1512.0 + 960.0),
            (rect(3432.0, 0.0, 3008.0, 1692.0), 3432.0 + 1504.0),
        ] {
            let (x, _) = top_left_for(screen, WINDOW, 0.0);
            assert_eq!(x + WINDOW.0 / 2.0, centre);
            assert!(lands_inside(screen, WINDOW, 0.0));
        }
    }

    #[test]
    fn a_window_bigger_than_its_screen_pins_to_the_screens_top_left() {
        let small = rect(1512.0, 0.0, 800.0, 600.0);
        assert_eq!(top_left_for(small, WINDOW, 0.0), (1512.0, 600.0));
        // Never reaches back across the seam into the neighbour on its left.
        let (x, _) = top_left_for(small, (1400.0, 900.0), 0.0);
        assert!(x >= 1512.0);
    }

    #[test]
    fn a_margin_that_would_push_the_window_off_the_screen_is_clamped() {
        let (_, top) = top_left_for(built_in(), WINDOW, 500.0);
        assert_eq!(top, 720.0, "the bottom edge stays on the screen");
        let (_, top) = top_left_for(built_in(), WINDOW, -40.0);
        assert_eq!(top, 982.0, "a negative margin cannot lift it off the top");
    }

    fn screen(name: &str, frame: Rect) -> Screen {
        Screen {
            name: name.into(),
            frame,
            scale: 1.0,
            notch: None,
        }
    }

    #[test]
    fn the_pointer_belongs_to_the_screen_it_is_over() {
        let screens = [
            screen("Built-in", built_in()),
            screen("DELL", rect(1512.0, -98.0, 1920.0, 1080.0)),
        ];
        assert_eq!(locate(&screens, (700.0, 400.0)), Some(0));
        assert_eq!(locate(&screens, (2000.0, 400.0)), Some(1));
        assert_eq!(locate(&screens, (-5.0, 400.0)), None);
    }

    /// The notch is the top row of pixels, where the pointer reports
    /// `y == max_y`. A half-open rect that excludes the top edge says "no
    /// screen" there, so hovering the notch would fall back to the primary.
    #[test]
    fn the_cursor_on_the_very_top_edge_is_on_that_screen() {
        let screens = [
            screen("Built-in", built_in()),
            screen("DELL", rect(1512.0, -98.0, 1920.0, 1080.0)),
        ];
        assert_eq!(locate(&screens, (2500.0, 982.0)), Some(1));
        // The seam between side-by-side screens belongs to exactly one.
        assert_eq!(locate(&screens, (1512.0, 500.0)), Some(1));
    }
}
