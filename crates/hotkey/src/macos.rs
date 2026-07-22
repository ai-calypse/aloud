//! macOS backend: CGEventTap — the only way to see modifier-only holds and to
//! swallow events before the focused app gets them. Needs Accessibility.

use crate::model::Key;
use anyhow::{anyhow, Result};
use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, EventField,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

extern "C" {
    /// Re-arms a tap the system switched off. `core-graphics` wraps this as
    /// `CGEventTap::enable`, but the tap cannot be borrowed inside its own
    /// callback, so we call it on the raw port instead.
    fn CGEventTapEnable(tap: *const std::ffi::c_void, enable: bool);
}

/// macOS virtual keycodes for keys that report as flagsChanged, not keyDown.
const KEY_LEFT_COMMAND: i64 = 55;
const KEY_RIGHT_COMMAND: i64 = 54;
const KEY_LEFT_SHIFT: i64 = 56;
const KEY_RIGHT_SHIFT: i64 = 60;
const KEY_LEFT_OPTION: i64 = 58;
const KEY_RIGHT_OPTION: i64 = 61;
const KEY_LEFT_CONTROL: i64 = 59;
const KEY_RIGHT_CONTROL: i64 = 62;
const KEY_FN: i64 = 63;

pub fn key_from_keycode(code: i64) -> Option<Key> {
    Some(match code {
        KEY_LEFT_COMMAND => Key::LeftCommand,
        KEY_RIGHT_COMMAND => Key::RightCommand,
        KEY_LEFT_SHIFT => Key::LeftShift,
        KEY_RIGHT_SHIFT => Key::RightShift,
        KEY_LEFT_OPTION => Key::LeftOption,
        KEY_RIGHT_OPTION => Key::RightOption,
        KEY_LEFT_CONTROL => Key::LeftControl,
        KEY_RIGHT_CONTROL => Key::RightControl,
        KEY_FN => Key::Fn,
        53 => Key::Escape,
        49 => Key::Space,
        122 => Key::F(1),
        120 => Key::F(2),
        99 => Key::F(3),
        118 => Key::F(4),
        96 => Key::F(5),
        97 => Key::F(6),
        98 => Key::F(7),
        100 => Key::F(8),
        101 => Key::F(9),
        109 => Key::F(10),
        103 => Key::F(11),
        111 => Key::F(12),
        // Physical positions, not legends: keycode 12 is US-QWERTY's Q, which
        // AZERTY labels A. Bindings fire on the key pressed; only the settings
        // keycap prints the US letter. Windows VK codes follow the legend.
        // ponytail: fixed US table — the upgrade is UCKeyTranslate against the
        // current TIS layout, which must run on the main thread and be cached
        // (TIS/TSM aborts the process off the main queue) and re-read on layout
        // change. Not worth it for a keycap letter.
        0 => Key::Char('a'),
        11 => Key::Char('b'),
        8 => Key::Char('c'),
        2 => Key::Char('d'),
        14 => Key::Char('e'),
        3 => Key::Char('f'),
        5 => Key::Char('g'),
        4 => Key::Char('h'),
        34 => Key::Char('i'),
        38 => Key::Char('j'),
        40 => Key::Char('k'),
        37 => Key::Char('l'),
        46 => Key::Char('m'),
        45 => Key::Char('n'),
        31 => Key::Char('o'),
        35 => Key::Char('p'),
        12 => Key::Char('q'),
        15 => Key::Char('r'),
        1 => Key::Char('s'),
        17 => Key::Char('t'),
        32 => Key::Char('u'),
        9 => Key::Char('v'),
        13 => Key::Char('w'),
        7 => Key::Char('x'),
        16 => Key::Char('y'),
        6 => Key::Char('z'),
        _ => return None,
    })
}

/// Whether a modifier key is down — flagsChanged reports the key that moved but
/// not its direction.
///
/// Side-specific `NX_DEVICE*` masks from IOKit's `ev_keymap.h`, which occupy the
/// low bits of the same flags word and have no `CGEventFlags` constants. The
/// public `CGEventFlagShift` is set while *either* Shift is down, and one
/// phantom key-down leaves the capture recorder armed — swallowing the whole
/// keyboard — until its ten-second timeout.
fn modifier_is_down(key: Key, flags: CGEventFlags) -> bool {
    const L_CONTROL: u64 = 0x0000_0001;
    const L_SHIFT: u64 = 0x0000_0002;
    const R_SHIFT: u64 = 0x0000_0004;
    const L_COMMAND: u64 = 0x0000_0008;
    const R_COMMAND: u64 = 0x0000_0010;
    const L_OPTION: u64 = 0x0000_0020;
    const R_OPTION: u64 = 0x0000_0040;
    const R_CONTROL: u64 = 0x0000_2000;

    let mask = match key {
        Key::LeftCommand => L_COMMAND,
        Key::RightCommand => R_COMMAND,
        Key::LeftShift => L_SHIFT,
        Key::RightShift => R_SHIFT,
        Key::LeftOption => L_OPTION,
        Key::RightOption => R_OPTION,
        Key::LeftControl => L_CONTROL,
        Key::RightControl => R_CONTROL,
        // Fn has no side, so the public flag is already exact.
        Key::Fn => return flags.contains(CGEventFlags::CGEventFlagSecondaryFn),
        _ => return false,
    };
    flags.bits() & mask != 0
}

/// Decoded event: the key and whether it went down.
pub struct RawEvent {
    pub key: Key,
    pub down: bool,
}

pub fn decode(event_type: CGEventType, event: &CGEvent) -> Option<RawEvent> {
    let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
    let key = key_from_keycode(code)?;

    match event_type {
        CGEventType::KeyDown => Some(RawEvent { key, down: true }),
        CGEventType::KeyUp => Some(RawEvent { key, down: false }),
        CGEventType::FlagsChanged => Some(RawEvent {
            down: modifier_is_down(key, event.get_flags()),
            key,
        }),
        _ => None,
    }
}

/// Whether the process is trusted for Accessibility, which the tap requires.
pub fn is_trusted() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

/// Runs a CGEventTap on the current thread's run loop. Blocks forever.
pub fn run_tap<F>(on_event: F) -> Result<()>
where
    F: FnMut(Option<RawEvent>) -> bool + 'static,
{
    // The tap callback is `Fn`, but the handler owns the matcher state and so
    // must be `FnMut`. The run loop calls it on one thread, serially.
    let on_event = RefCell::new(on_event);

    // Filled in after the tap exists. macOS disables a slow or interrupted tap
    // via TapDisabledBy*, and without re-arming it every hotkey stays dead until
    // relaunch. The tap outlives this closure, so the raw port stays valid.
    let port: Rc<Cell<*const std::ffi::c_void>> = Rc::new(Cell::new(std::ptr::null()));
    let port_cb = port.clone();

    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default, // must be Default (not ListenOnly) to swallow events
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::FlagsChanged,
        ],
        move |_proxy, event_type, event| {
            if matches!(
                event_type,
                CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
            ) {
                let port = port_cb.get();
                if !port.is_null() {
                    tracing::warn!(?event_type, "event tap disabled by the system; re-enabling");
                    unsafe { CGEventTapEnable(port, true) };
                }
                return Some(event.to_owned());
            }

            // Never let our own synthetic keystrokes drive our own hotkeys: a
            // Session tap sees `aloud_paste`'s Cmd+C/Cmd+V as the user's own,
            // which loops Activated/Deactivated for as long as a rewrite key is
            // held. The pid is the reliable discriminator — real hardware
            // reports 0, a posted event carries its poster's pid.
            if event.get_integer_value_field(EventField::EVENT_SOURCE_UNIX_PROCESS_ID)
                == i64::from(std::process::id())
            {
                return Some(event.to_owned());
            }

            // `None` for an unmapped key, but the handler still gets a say:
            // while recording a binding *every* key must be swallowed, or the
            // Return of a Cmd+Return submits whatever is focused underneath.
            let swallow = (on_event.borrow_mut())(decode(event_type, event));
            // Returning None swallows the event.
            if swallow {
                None
            } else {
                Some(event.to_owned())
            }
        },
    )
    .map_err(|_| anyhow!("failed to create event tap — is Accessibility permission granted?"))?;

    port.set(tap.mach_port.as_concrete_TypeRef() as *const std::ffi::c_void);

    let loop_source = tap
        .mach_port
        .create_runloop_source(0)
        .map_err(|_| anyhow!("failed to create run loop source"))?;

    let run_loop = CFRunLoop::get_current();
    unsafe { run_loop.add_source(&loop_source, kCFRunLoopCommonModes) };
    tap.enable();

    tracing::info!("event tap running");
    CFRunLoop::run_current();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_modifier_keycodes_to_sides() {
        assert_eq!(key_from_keycode(61), Some(Key::RightOption));
        assert_eq!(key_from_keycode(58), Some(Key::LeftOption));
        assert_eq!(key_from_keycode(63), Some(Key::Fn));
        assert_eq!(key_from_keycode(53), Some(Key::Escape));
        assert_eq!(key_from_keycode(2), Some(Key::Char('d')));
        assert_eq!(key_from_keycode(9999), None);
    }
}
