//! The built-in display's brightness, for the notch's replacement of the
//! brightness HUD: read it, and move it the way the brightness keys do.
//!
//! `DisplayServices` is a private framework (it is what System Settings and
//! the keys themselves call), so it is `dlopen`ed when first asked and never
//! linked: a macOS that drops it makes every call here say "not controllable"
//! and the key falls through to macOS.
//!
//! ponytail: the Mac's own panel only. An external display's brightness is
//! DDC/CI over IOAVService (Apple Silicon) or IOI2C (Intel): a second private
//! surface per architecture, which MonitorControl exists to carry. When the
//! main display is external, the key is left to macOS.

use anyhow::{bail, Result};

/// One key press, from `from` (0.0–1.0). A step is 1/16, or 1/64 with
/// Option+Shift, and lands on the grid: from 0.53 one press up is 0.5625, as
/// macOS does, not 0.5925. The same grid as the volume keys
/// (`aloud_media::sound::step_output`); this crate does not depend on media for
/// ten lines of arithmetic.
pub fn step(from: f32, up: bool, fine: bool) -> f32 {
    let step = if fine { 1.0 / 64.0 } else { 1.0 / 16.0 };
    // A hair of slack so a level already on the grid (stored as an f32) moves
    // a whole step instead of snapping to itself.
    const SLACK: f32 = 1e-3;
    let slots = from / step;
    if up {
        (((slots + SLACK).floor() + 1.0) * step).min(1.0)
    } else {
        (((slots - SLACK).ceil() - 1.0) * step).max(0.0)
    }
}

/// 0–100 for the bar.
pub fn percent(level: f32) -> u8 {
    (level.clamp(0.0, 1.0) * 100.0).round() as u8
}

#[cfg(target_os = "macos")]
pub use imp::{controllable, get, nudge, set};

#[cfg(not(target_os = "macos"))]
pub fn controllable() -> bool {
    false
}
#[cfg(not(target_os = "macos"))]
pub fn get() -> Result<f32> {
    bail!("brightness is macOS only")
}
#[cfg(not(target_os = "macos"))]
pub fn set(_level: f32) -> Result<()> {
    bail!("brightness is macOS only")
}
#[cfg(not(target_os = "macos"))]
pub fn nudge(_up: bool, _fine: bool) -> Result<f32> {
    bail!("brightness is macOS only")
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::ffi::{c_char, c_int, c_void};
    use std::sync::OnceLock;

    type GetFn = unsafe extern "C" fn(u32, *mut f32) -> i32;
    type SetFn = unsafe extern "C" fn(u32, f32) -> i32;

    struct Api {
        get: GetFn,
        set: SetFn,
    }

    // libSystem, already in every process: no link attribute needed.
    extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayIsBuiltin(display: u32) -> u32;
    }

    const RTLD_LAZY: c_int = 1;

    fn api() -> Option<&'static Api> {
        static API: OnceLock<Option<Api>> = OnceLock::new();
        API.get_or_init(|| {
            // SAFETY: NUL-terminated literals; a null handle or symbol is
            // checked before use. The handle is never closed, so the
            // pointers stay valid for the process.
            unsafe {
                let lib = dlopen(
                    c"/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices"
                        .as_ptr(),
                    RTLD_LAZY,
                );
                if lib.is_null() {
                    return None;
                }
                let get = dlsym(lib, c"DisplayServicesGetBrightness".as_ptr());
                let set = dlsym(lib, c"DisplayServicesSetBrightness".as_ptr());
                if get.is_null() || set.is_null() {
                    return None;
                }
                Some(Api {
                    get: std::mem::transmute::<*mut c_void, GetFn>(get),
                    set: std::mem::transmute::<*mut c_void, SetFn>(set),
                })
            }
        })
        .as_ref()
    }

    /// The display the user is on, when it is the Mac's own panel. With the
    /// lid closed the main display is the external one, and so it is None.
    fn builtin_main() -> Option<u32> {
        // SAFETY: plain queries, no pointers.
        let main = unsafe { CGMainDisplayID() };
        (unsafe { CGDisplayIsBuiltin(main) } != 0).then_some(main)
    }

    /// Whether a brightness key should be eaten: the framework loads and the
    /// built-in display is the main one. Cheap enough to ask the event tap on
    /// every key press (no display I/O).
    pub fn controllable() -> bool {
        api().is_some() && builtin_main().is_some()
    }

    pub fn get() -> Result<f32> {
        let (Some(api), Some(id)) = (api(), builtin_main()) else {
            bail!("the built-in display's brightness is not controllable");
        };
        let mut level = 0.0f32;
        // SAFETY: `level` is a live f32 for the call.
        let status = unsafe { (api.get)(id, &mut level) };
        if status != 0 {
            bail!("DisplayServicesGetBrightness returned {status}");
        }
        Ok(level.clamp(0.0, 1.0))
    }

    pub fn set(level: f32) -> Result<()> {
        let (Some(api), Some(id)) = (api(), builtin_main()) else {
            bail!("the built-in display's brightness is not controllable");
        };
        // SAFETY: plain call with a display id the system just returned.
        let status = unsafe { (api.set)(id, level.clamp(0.0, 1.0)) };
        if status != 0 {
            bail!("DisplayServicesSetBrightness returned {status}");
        }
        Ok(())
    }

    /// One key press: the new level, 0.0–1.0.
    pub fn nudge(up: bool, fine: bool) -> Result<f32> {
        let to = step(get()?, up, fine);
        set(to)?;
        Ok(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_lands_on_the_grid_like_macos() {
        // The failure this guards: adding 1/16 to an off-grid level, so the
        // bar drifts away from the notches the keyboard's other HUD uses.
        assert!((step(0.53, true, false) - 0.5625).abs() < 1e-6);
        assert!((step(0.53, false, false) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_level_already_on_the_grid_moves_a_whole_step() {
        assert!((step(0.5, true, false) - 0.5625).abs() < 1e-6);
        assert!((step(0.5, false, false) - 0.4375).abs() < 1e-6);
        assert!((step(0.5, true, true) - (0.5 + 1.0 / 64.0)).abs() < 1e-6);
    }

    #[test]
    fn the_ends_hold() {
        assert_eq!(step(1.0, true, false), 1.0);
        assert_eq!(step(0.0, false, false), 0.0);
        assert_eq!(step(0.0, true, false), 1.0 / 16.0);
    }

    #[test]
    fn the_bar_is_whole_percent() {
        assert_eq!(percent(0.5625), 56);
        assert_eq!(percent(1.2), 100);
        assert_eq!(percent(-0.1), 0);
    }
}
