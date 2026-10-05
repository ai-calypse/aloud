//! External display connect and disconnect from CoreGraphics.
//!
//! One reconfiguration is several callbacks (a begin marker, then a flagged
//! one per display), so the callback only decides *whether to look*; what it
//! looks at is the count of online non-built-in displays, settled for a
//! moment. Counting the state instead of decoding each flag is what keeps
//! mirroring, rearranging and wake-from-sleep from reading as a plug.

use std::ffi::c_void;
use std::sync::Arc;
use std::time::Duration;

use super::{Emit, Event, Settle};

const SETTLE: Duration = Duration::from_millis(1500);

const BEGIN_CONFIGURATION: u32 = 1 << 0;
const ADD: u32 = 1 << 4;
const REMOVE: u32 = 1 << 5;
const ENABLED: u32 = 1 << 8;
const DISABLED: u32 = 1 << 9;

/// A callback worth re-counting for: a display arrived, left, or switched.
/// The begin marker is the state *before* the change, so it never is.
pub fn worth_counting(flags: u32) -> bool {
    flags & BEGIN_CONFIGURATION == 0 && flags & (ADD | REMOVE | ENABLED | DISABLED) != 0
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGGetOnlineDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayIsBuiltin(display: u32) -> u32;
    fn CGDisplayRegisterReconfigurationCallback(
        callback: extern "C" fn(u32, u32, *mut c_void),
        user_info: *mut c_void,
    ) -> i32;
}

/// Online displays that are not the Mac's own panel. Online, not active: a
/// mirrored or asleep display is still plugged in.
pub fn external_count() -> u32 {
    let mut ids = [0u32; 32];
    let mut n = 0u32;
    // SAFETY: `ids` holds the 32 the call is told it may fill.
    if unsafe { CGGetOnlineDisplayList(32, ids.as_mut_ptr(), &mut n) } != 0 {
        return 0;
    }
    ids[..n as usize]
        .iter()
        // SAFETY: a plain query on an id the system just returned.
        .filter(|&&id| unsafe { CGDisplayIsBuiltin(id) } == 0)
        .count() as u32
}

struct State {
    emit: Emit,
    settle: Arc<Settle<u32>>,
}

extern "C" fn reconfigured(_display: u32, flags: u32, context: *mut c_void) {
    if !worth_counting(flags) {
        return;
    }
    // SAFETY: the context is the leaked `State` from `install`, never freed.
    let s = unsafe { &*(context as *const State) };
    let emit = s.emit.clone();
    s.settle.observe(external_count(), move |from, to| {
        emit(Event::ExternalDisplay(to > from));
    });
}

pub fn install(emit: Emit) {
    let state = Box::new(State {
        emit,
        settle: Settle::new(external_count(), SETTLE),
    });
    // ponytail: leaked for the process's life — watchers are never uninstalled.
    let context = Box::into_raw(state).cast();
    // SAFETY: the callback is `extern "C"` and the context outlives it (leaked).
    let status = unsafe { CGDisplayRegisterReconfigurationCallback(reconfigured, context) };
    if status != 0 {
        tracing::warn!(status, "could not subscribe to display changes");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The failure: counting on the begin marker reads the display list
    /// mid-change and whispers for a state that is about to be replaced.
    #[test]
    fn the_begin_marker_is_not_worth_counting() {
        assert!(!worth_counting(BEGIN_CONFIGURATION));
        assert!(!worth_counting(BEGIN_CONFIGURATION | ADD));
    }

    #[test]
    fn a_display_arriving_leaving_or_switching_is() {
        for f in [ADD, REMOVE, ENABLED, DISABLED, ADD | ENABLED] {
            assert!(worth_counting(f), "{f:#x}");
        }
    }

    #[test]
    fn a_move_or_mode_change_alone_is_not() {
        let moved = 1 << 1;
        let set_mode = 1 << 3;
        assert!(!worth_counting(moved | set_mode));
        assert!(!worth_counting(0));
    }
}
