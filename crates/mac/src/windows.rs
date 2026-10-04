//! Snapping the window the user was in — left, right, halves, quarters,
//! maximise — through the Accessibility API.
//!
//! The window is found without remembering anything: `CGWindowList` lists
//! on-screen windows front to back, and the first ordinary one that is not
//! Aloud's is the window the user was in, even while Aloud holds key focus
//! for a click on the notch. Owner names and bounds need no Screen Recording
//! permission (titles would).
//!
//! `aloud_native`'s `ALOUD_WINDOW_ARRANGE` does the same through AppleScript
//! for the voice agent; this takes ~10ms instead of ~300ms, and reads the
//! real usable area per display (Dock and menu bar excluded) rather than
//! assuming a 25pt menu bar.

use serde::{Deserialize, Serialize};

/// Points, origin top-left of the primary display — the Accessibility API's
/// own space, so nothing converts on the way in.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn overlap(&self, other: &Rect) -> f64 {
        let w = (self.x + self.w).min(other.x + other.w) - self.x.max(other.x);
        let h = (self.y + self.h).min(other.y + other.h) - self.y.max(other.y);
        if w > 0.0 && h > 0.0 {
            w * h
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Snap {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Maximize,
    Center,
}

/// Where `snap` puts a window on a screen whose usable area is `v`. Halves
/// split on whole points, the right and bottom taking the odd one, so two
/// snapped windows neither overlap nor leave a seam.
pub fn snap_rect(snap: Snap, v: Rect) -> Rect {
    let left_w = (v.w / 2.0).floor();
    let top_h = (v.h / 2.0).floor();
    let (x, y, w, h) = match snap {
        Snap::Maximize => (v.x, v.y, v.w, v.h),
        Snap::Left => (v.x, v.y, left_w, v.h),
        Snap::Right => (v.x + left_w, v.y, v.w - left_w, v.h),
        Snap::Top => (v.x, v.y, v.w, top_h),
        Snap::Bottom => (v.x, v.y + top_h, v.w, v.h - top_h),
        Snap::TopLeft => (v.x, v.y, left_w, top_h),
        Snap::TopRight => (v.x + left_w, v.y, v.w - left_w, top_h),
        Snap::BottomLeft => (v.x, v.y + top_h, left_w, v.h - top_h),
        Snap::BottomRight => (v.x + left_w, v.y + top_h, v.w - left_w, v.h - top_h),
        Snap::Center => {
            let (w, h) = ((v.w * 0.6).round(), (v.h * 0.75).round());
            (
                v.x + ((v.w - w) / 2.0).floor(),
                v.y + ((v.h - h) / 2.0).floor(),
                w,
                h,
            )
        }
    };
    Rect { x, y, w, h }
}

/// The screen a window is mostly on, so a snap stays on its display. A window
/// wholly off every screen (mid-drag, or a display just unplugged) takes the
/// first, which is the primary.
pub fn pick_screen(window: Rect, screens: &[Rect]) -> Option<Rect> {
    screens
        .iter()
        .copied()
        .max_by(|a, b| window.overlap(a).total_cmp(&window.overlap(b)))
}

/// A Cocoa frame (origin bottom-left of the primary display, y up) in the
/// Accessibility space (origin top-left, y down). `primary_height` is the
/// primary screen's frame height, the one both spaces hang from.
pub fn cocoa_to_ax(primary_height: f64, frame: Rect) -> Rect {
    Rect {
        y: primary_height - frame.y - frame.h,
        ..frame
    }
}

/// The window the user was in.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontWindow {
    pub app: String,
    pub pid: i32,
    pub bounds: Rect,
}

/// Windows smaller than this are palettes and popovers, not what a person
/// means by "snap this".
const MIN_SIDE: f64 = 120.0;

/// One entry of the on-screen window list, as the choice below sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub app: String,
    pub pid: i32,
    pub layer: i64,
    pub alpha: f64,
    pub bounds: Rect,
}

/// The windows in a front-to-back list that could be what the user was in:
/// ordinary, visible, sizeable, and not Aloud's. Still front to back — the
/// caller takes the first that Accessibility agrees is a real window, since
/// some apps keep a screen-sized, see-through layer-0 window that no list
/// field gives away.
pub fn candidates(list: impl IntoIterator<Item = Listed>, own_pid: i32) -> Vec<FrontWindow> {
    list.into_iter()
        // Layer 0 is document windows; the menu bar, Dock and every overlay
        // (Aloud's included) sit above it.
        .filter(|w| {
            w.layer == 0
                && w.pid != own_pid
                && w.alpha > 0.0
                && w.bounds.w >= MIN_SIDE
                && w.bounds.h >= MIN_SIDE
        })
        .map(|w| FrontWindow {
            app: w.app,
            pid: w.pid,
            bounds: w.bounds,
        })
        .collect()
}

#[cfg(target_os = "macos")]
pub use macos::{front_window, is_trusted, snap, visible_screens};

/// Off macOS there is nothing to snap through.
#[cfg(not(target_os = "macos"))]
pub fn front_window() -> Option<FrontWindow> {
    None
}
#[cfg(not(target_os = "macos"))]
pub fn is_trusted() -> bool {
    false
}
#[cfg(not(target_os = "macos"))]
pub fn visible_screens() -> Vec<Rect> {
    Vec::new()
}
#[cfg(not(target_os = "macos"))]
pub fn snap(_: Snap, _: &[Rect]) -> anyhow::Result<String> {
    anyhow::bail!("window snapping is macOS only")
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;

    use anyhow::{bail, Context, Result};
    use core_foundation::array::{CFArray, CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::{CFString, CFStringRef};

    use super::{candidates, cocoa_to_ax, pick_screen, snap_rect, FrontWindow, Listed, Rect, Snap};

    type AxElement = *const c_void;

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct CgRect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }

    const AX_SUCCESS: i32 = 0;
    const AX_POINT: u32 = 1;
    const AX_SIZE: u32 = 2;
    /// `kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements`.
    const ON_SCREEN_WINDOWS: u32 = 1 | (1 << 4);

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXUIElementCreateApplication(pid: i32) -> AxElement;
        fn AXUIElementSetMessagingTimeout(element: AxElement, seconds: f32) -> i32;
        fn AXUIElementCopyAttributeValue(
            element: AxElement,
            attr: CFStringRef,
            out: *mut *const c_void,
        ) -> i32;
        fn AXUIElementSetAttributeValue(
            element: AxElement,
            attr: CFStringRef,
            value: *const c_void,
        ) -> i32;
        fn AXValueCreate(kind: u32, value: *const c_void) -> *const c_void;
        fn CGWindowListCopyWindowInfo(options: u32, relative_to: u32) -> CFArrayRef;
        fn CGRectMakeWithDictionaryRepresentation(dict: CFDictionaryRef, rect: *mut CgRect)
            -> bool;
    }

    pub fn is_trusted() -> bool {
        // SAFETY: no arguments, no state.
        unsafe { AXIsProcessTrusted() }
    }

    /// Usable areas of every display, Accessibility space. **Main thread
    /// only** (NSScreen); empty anywhere else, which the caller reports.
    pub fn visible_screens() -> Vec<Rect> {
        use objc2_app_kit::NSScreen;
        use objc2_foundation::MainThreadMarker;

        let Some(mtm) = MainThreadMarker::new() else {
            return Vec::new();
        };
        let screens = NSScreen::screens(mtm);
        let Some(primary) = screens.iter().next() else {
            return Vec::new();
        };
        let primary_height = primary.frame().size.height;
        screens
            .iter()
            .map(|s| {
                let v = s.visibleFrame();
                cocoa_to_ax(
                    primary_height,
                    Rect {
                        x: v.origin.x,
                        y: v.origin.y,
                        w: v.size.width,
                        h: v.size.height,
                    },
                )
            })
            .collect()
    }

    fn key(name: &str) -> CFString {
        CFString::new(name)
    }

    fn number(dict: &CFDictionary<CFString, CFType>, name: &str) -> Option<f64> {
        dict.find(key(name))?.downcast::<CFNumber>()?.to_f64()
    }

    /// The window the user was in; see the module doc.
    pub fn front_window() -> Option<FrontWindow> {
        found().map(|(front, _)| front)
    }

    /// Candidates in front-to-back order from the window server.
    fn listed() -> Vec<FrontWindow> {
        // SAFETY: returns a +1 array or null; wrapped to release it.
        let raw = unsafe { CGWindowListCopyWindowInfo(ON_SCREEN_WINDOWS, 0) };
        if raw.is_null() {
            return Vec::new();
        }
        // SAFETY: `raw` is a non-null CFArray we own.
        let list = unsafe { CFArray::<*const c_void>::wrap_under_create_rule(raw) };
        // SAFETY: indices are within the array's count; entries are
        // CFDictionaries owned by it, borrowed for this loop only.
        let count = unsafe { CFArrayGetCount(list.as_concrete_TypeRef()) };
        let entries = (0..count).filter_map(|i| {
            let dict = unsafe {
                let item = CFArrayGetValueAtIndex(list.as_concrete_TypeRef(), i);
                CFDictionary::<CFString, CFType>::wrap_under_get_rule(item as CFDictionaryRef)
            };
            let bounds = dict.find(key("kCGWindowBounds"))?;
            let mut rect = CgRect::default();
            // SAFETY: the bounds value is a CFDictionary by contract.
            let ok = unsafe {
                CGRectMakeWithDictionaryRepresentation(
                    bounds.as_CFTypeRef() as CFDictionaryRef,
                    &mut rect,
                )
            };
            ok.then_some(())?;
            let app = dict
                .find(key("kCGWindowOwnerName"))?
                .downcast::<CFString>()?
                .to_string();
            Some(Listed {
                app,
                pid: number(&dict, "kCGWindowOwnerPID")? as i32,
                layer: number(&dict, "kCGWindowLayer")? as i64,
                alpha: number(&dict, "kCGWindowAlpha").unwrap_or(1.0),
                bounds: Rect {
                    x: rect.x,
                    y: rect.y,
                    w: rect.w,
                    h: rect.h,
                },
            })
        });
        candidates(entries, std::process::id() as i32)
    }

    /// The first candidate whose app exposes a standard window through
    /// Accessibility, with that window's element.
    fn found() -> Option<(FrontWindow, CFType)> {
        listed().into_iter().find_map(|front| {
            let window = standard_window(front.pid)?;
            Some((front, window))
        })
    }

    fn standard_window(pid: i32) -> Option<CFType> {
        // SAFETY: a live pid's application element, owned +1.
        let app = unsafe { AXUIElementCreateApplication(pid) };
        if app.is_null() {
            return None;
        }
        let app = unsafe { CFType::wrap_under_create_rule(app) };
        // A hung app answers in the timeout, not the default six seconds.
        unsafe { AXUIElementSetMessagingTimeout(app.as_CFTypeRef(), 1.0) };
        let standard = |w: &CFType| {
            attribute(w, "AXSubrole")
                .and_then(|r| r.downcast::<CFString>())
                .is_some_and(|r| matches!(r.to_string().as_str(), "AXStandardWindow"))
        };
        attribute(&app, "AXFocusedWindow")
            .filter(standard)
            .or_else(|| {
                let windows = attribute(&app, "AXWindows")?;
                // SAFETY: AXWindows is a CFArray of elements, retained by the wrap.
                let windows = unsafe {
                    CFArray::<*const c_void>::wrap_under_get_rule(
                        windows.as_CFTypeRef() as CFArrayRef
                    )
                };
                (0..windows.len()).find_map(|i| {
                    let item = unsafe { CFArrayGetValueAtIndex(windows.as_concrete_TypeRef(), i) };
                    let w = unsafe { CFType::wrap_under_get_rule(item) };
                    standard(&w).then_some(w)
                })
            })
    }

    /// Moves the front window to `snap` on the display it is mostly on.
    /// Returns the app's name. `screens` come from [`visible_screens`], read
    /// on the main thread, because this runs wherever a blocked Accessibility
    /// call cannot freeze the UI.
    pub fn snap(snap: Snap, screens: &[Rect]) -> Result<String> {
        if !is_trusted() {
            bail!("Aloud needs Accessibility access to move windows");
        }
        let (front, window) = found().context("no window to snap")?;
        let screen = pick_screen(front.bounds, screens).context("could not read the displays")?;
        let target = snap_rect(snap, screen);

        // Size, position, size: a window moving to a smaller display is
        // clamped to the old size on the first resize, and one growing past
        // its screen edge is clamped by the move.
        set_size(&window, target)?;
        set_position(&window, target)?;
        set_size(&window, target)?;
        Ok(front.app)
    }

    fn attribute(element: &CFType, name: &str) -> Option<CFType> {
        let mut out: *const c_void = std::ptr::null();
        // SAFETY: a +1 value on success.
        let err = unsafe {
            AXUIElementCopyAttributeValue(
                element.as_CFTypeRef(),
                key(name).as_concrete_TypeRef(),
                &mut out,
            )
        };
        (err == AX_SUCCESS && !out.is_null())
            .then(|| unsafe { CFType::wrap_under_create_rule(out) })
    }

    fn set(window: &CFType, name: &str, kind: u32, pair: [f64; 2]) -> Result<()> {
        // SAFETY: `pair` is two f64s, CGPoint and CGSize's layout.
        let value = unsafe { AXValueCreate(kind, pair.as_ptr().cast()) };
        if value.is_null() {
            bail!("could not build the {name} value");
        }
        let value = unsafe { CFType::wrap_under_create_rule(value) };
        let err = unsafe {
            AXUIElementSetAttributeValue(
                window.as_CFTypeRef(),
                key(name).as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            )
        };
        if err != AX_SUCCESS {
            bail!("the window refused {name} (error {err})");
        }
        Ok(())
    }

    fn set_size(window: &CFType, r: Rect) -> Result<()> {
        set(window, "AXSize", AX_SIZE, [r.w, r.h])
    }

    fn set_position(window: &CFType, r: Rect) -> Result<()> {
        set(window, "AXPosition", AX_POINT, [r.x, r.y])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0.0,
        y: 38.0,
        w: 1511.0,
        h: 944.0,
    };

    #[test]
    fn halves_and_quarters_tile_the_screen_without_a_seam_on_odd_widths() {
        let left = snap_rect(Snap::Left, SCREEN);
        let right = snap_rect(Snap::Right, SCREEN);
        assert_eq!(left.x + left.w, right.x, "no gap between the halves");
        assert_eq!(right.x + right.w, SCREEN.x + SCREEN.w, "reaches the edge");
        let (tl, br) = (
            snap_rect(Snap::TopLeft, SCREEN),
            snap_rect(Snap::BottomRight, SCREEN),
        );
        assert_eq!(tl.y + tl.h, br.y);
        assert_eq!(
            (br.x + br.w, br.y + br.h),
            (SCREEN.x + SCREEN.w, SCREEN.y + SCREEN.h)
        );
        assert_eq!(snap_rect(Snap::Maximize, SCREEN), SCREEN);
        let c = snap_rect(Snap::Center, SCREEN);
        assert!(c.x > 0.0 && c.x + c.w < SCREEN.w && c.y > SCREEN.y);
    }

    /// The failure: snapping onto the primary display whatever display the
    /// window was on, which dragged it across the desk.
    #[test]
    fn a_snap_stays_on_the_display_the_window_is_mostly_on() {
        let primary = Rect {
            x: 0.0,
            y: 38.0,
            w: 1512.0,
            h: 944.0,
        };
        let external = Rect {
            x: 1512.0,
            y: -200.0,
            w: 2560.0,
            h: 1415.0,
        };
        let window = Rect {
            x: 1400.0,
            y: 100.0,
            w: 900.0,
            h: 700.0,
        };
        // 112pt on the primary, 788pt on the external.
        assert_eq!(pick_screen(window, &[primary, external]), Some(external));
        let off = Rect {
            x: 9000.0,
            y: 0.0,
            w: 300.0,
            h: 300.0,
        };
        assert!(pick_screen(off, &[primary, external]).is_some());
        assert_eq!(pick_screen(window, &[]), None);
    }

    #[test]
    fn cocoa_frames_flip_into_accessibility_space() {
        // Primary 1117 tall: menu bar 38 at the top, Dock 70 at the bottom.
        let visible = Rect {
            x: 0.0,
            y: 70.0,
            w: 1728.0,
            h: 1009.0,
        };
        assert_eq!(cocoa_to_ax(1117.0, visible).y, 38.0);
        // A display above the primary has a negative AX y.
        let above = Rect {
            x: 0.0,
            y: 1117.0,
            w: 1920.0,
            h: 1055.0,
        };
        assert_eq!(cocoa_to_ax(1117.0, above).y, -1055.0);
    }

    fn listed(app: &str, pid: i32, layer: i64, w: f64) -> Listed {
        Listed {
            app: app.into(),
            pid,
            layer,
            alpha: 1.0,
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                w,
                h: 600.0,
            },
        }
    }

    /// The failure: snapping Aloud's own overlay, or the menu bar, because
    /// they are the front-most windows while the notch is open.
    #[test]
    fn candidates_skip_aloud_overlays_and_palettes() {
        let list = vec![
            listed("Aloud", 50, 0, 780.0),
            listed("Window Server", 1, 25, 1512.0),
            listed("Safari", 7, 0, 60.0),
            listed("Safari", 7, 0, 1000.0),
            listed("Mail", 9, 0, 900.0),
        ];
        let apps: Vec<_> = candidates(list, 50)
            .into_iter()
            .map(|w| (w.app, w.pid))
            .collect();
        assert_eq!(apps, [("Safari".to_string(), 7), ("Mail".to_string(), 9)]);
        assert!(candidates(Vec::new(), 50).is_empty());
    }
}
