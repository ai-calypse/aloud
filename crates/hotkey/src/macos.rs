//! macOS backend: CGEventTap — the only way to see modifier-only holds and to
//! swallow events before the focused app gets them. Needs Accessibility.

use crate::aux;
use crate::model::Key;
use anyhow::{anyhow, Result};
use core_foundation::base::TCFType;
use core_foundation::mach_port::CFMachPort;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventType, EventField};
use foreign_types::ForeignType;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;

extern "C" {
    /// Re-arms a tap the system switched off, on the raw port: the tap cannot
    /// be borrowed inside its own callback.
    fn CGEventTapEnable(tap: *const c_void, enable: bool);
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

/// The tap's event kinds as raw numbers. `core-graphics`' `CGEventType` has no
/// NX_SYSDEFINED, and a callback handed an unlisted discriminant is undefined
/// behaviour, so the tap is made here, on the raw API, and kinds stay `u32`.
const KIND_KEY_DOWN: u32 = 10;
const KIND_KEY_UP: u32 = 11;
const KIND_FLAGS_CHANGED: u32 = 12;
const KIND_SYSTEM_DEFINED: u32 = 14;
const KIND_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const KIND_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

const KEY_CAPS_LOCK: i64 = 57;
/// `kCGEventFlagMaskAlphaShift`: set while Caps Lock is on.
const FLAG_ALPHA_SHIFT: u64 = 0x0001_0000;

type RawEventRef = core_graphics::sys::CGEventRef;
type TapCallback = unsafe extern "C" fn(
    proxy: *const c_void,
    kind: u32,
    event: RawEventRef,
    user: *mut c_void,
) -> RawEventRef;

extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        mask: u64,
        callback: TapCallback,
        user: *mut c_void,
    ) -> *mut c_void;
    /// 0 is `kCGEventSourceStateCombinedSessionState`.
    fn CGEventSourceFlagsState(state: i32) -> u64;
}

// `NSEvent` is the only public way to read NX_SYSDEFINED's `data1`; AppKit
// must be linked for the class to exist in a binary that never uses it.
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

/// `CGEventRef` as Objective-C encodes it, so debug builds' message
/// verification accepts it as the argument of `eventWithCGEvent:`.
#[repr(transparent)]
struct EventArg(RawEventRef);
// SAFETY: a `CGEventRef` is a pointer to an opaque struct named `__CGEvent`.
unsafe impl objc2::Encode for EventArg {
    const ENCODING: objc2::Encoding =
        objc2::Encoding::Pointer(&objc2::Encoding::Struct("__CGEvent", &[]));
}

/// Reads the volume key out of an NX_SYSDEFINED event, or None for any other
/// system-defined event.
fn read_system_key(event: &CGEvent) -> Option<aux::SystemKey> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    // `eventWithCGEvent:` returns an autoreleased object, and this thread has
    // no pool of its own.
    objc2::rc::autoreleasepool(|_| {
        // SAFETY: `event` is live for the call; NSEvent copies what it needs
        // and a nil result (not an NSEvent) is checked before use.
        unsafe {
            let ns: *mut AnyObject =
                msg_send![class!(NSEvent), eventWithCGEvent: EventArg(event.as_ptr())];
            if ns.is_null() {
                return None;
            }
            let subtype: i16 = msg_send![ns, subtype];
            let data1: isize = msg_send![ns, data1];
            aux::decode_system_key(subtype, data1 as i64)
        }
    })
}

/// What the tap callback owns. Boxed and leaked to the C callback for as long
/// as the run loop runs, which is the life of the thread.
struct Tap {
    // The tap callback is `Fn`, but the handler owns the matcher state and so
    // must be `FnMut`. The run loop calls it on one thread, serially.
    on_event: RefCell<Box<dyn FnMut(Option<RawEvent>) -> bool>>,
    // Filled in after the tap exists. macOS disables a slow or interrupted tap
    // via TapDisabledBy*, and without re-arming it every hotkey stays dead until
    // relaunch. The tap outlives the callback, so the raw port stays valid.
    port: Cell<*const c_void>,
}

impl Tap {
    /// Whether to swallow the event.
    fn handle(&self, kind: u32, event: &CGEvent) -> bool {
        if matches!(kind, KIND_DISABLED_BY_TIMEOUT | KIND_DISABLED_BY_USER_INPUT) {
            let port = self.port.get();
            if !port.is_null() {
                tracing::warn!(kind, "event tap disabled by the system; re-enabling");
                unsafe { CGEventTapEnable(port, true) };
            }
            return false;
        }

        // Never let our own synthetic keystrokes drive our own hotkeys: a
        // Session tap sees `aloud_paste`'s Cmd+C/Cmd+V as the user's own,
        // which loops Activated/Deactivated for as long as a rewrite key is
        // held. The pid is the reliable discriminator — real hardware
        // reports 0, a posted event carries its poster's pid.
        if event.get_integer_value_field(EventField::EVENT_SOURCE_UNIX_PROCESS_ID)
            == i64::from(std::process::id())
        {
            return false;
        }

        let event_type = match kind {
            KIND_KEY_DOWN => CGEventType::KeyDown,
            KIND_KEY_UP => CGEventType::KeyUp,
            KIND_FLAGS_CHANGED => CGEventType::FlagsChanged,
            KIND_SYSTEM_DEFINED => return self.system_defined(event),
            _ => return false,
        };

        // Caps Lock is a state, not a hold: it never drives the matcher, and
        // a bare modifier is not typing. Passed through untouched.
        if kind == KIND_FLAGS_CHANGED
            && event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) == KEY_CAPS_LOCK
        {
            let on = event.get_flags().bits() & FLAG_ALPHA_SHIFT != 0;
            if let Some(edge) = aux::caps_edge(on) {
                aux::emit(edge);
            }
            return false;
        }

        // `None` for an unmapped key, but the handler still gets a say: it
        // owns the matcher, the capture recorder and the typing signal.
        //
        // Its swallow verdict is not applied. Until the tap moved onto the raw
        // API, `core-graphics` 0.24 turned `None` into "pass the original
        // event", so the matcher's `Consume::Yes` and capture's swallow-all
        // never ate a key, and the app shipped that way. Honouring them now
        // would change every dictation binding at once, unverified, as a side
        // effect of the volume keys; only the volume keys swallow.
        // ponytail: apply it (`return verdict`) once Consume::Yes is exercised
        // on hardware, binding by binding.
        let _verdict = (self.on_event.borrow_mut())(decode(event_type, event));
        false
    }

    /// A volume key. Never reaches the dictation handler.
    fn system_defined(&self, event: &CGEvent) -> bool {
        let Some(sys) = read_system_key(event) else {
            return false;
        };
        let flags = event.get_flags();
        let option = flags.contains(CGEventFlags::CGEventFlagAlternate);
        let shift = flags.contains(CGEventFlags::CGEventFlagShift);
        let swallow = aux::swallows_volume(option && !shift);
        // The key-up is eaten too, or the focused app sees half a press; only
        // the down is an event.
        if sys.down {
            aux::emit(aux::AuxEvent::Volume {
                key: sys.key,
                fine: option && shift,
                swallowed: swallow,
            });
        }
        swallow
    }
}

unsafe extern "C" fn trampoline(
    _proxy: *const c_void,
    kind: u32,
    event: RawEventRef,
    user: *mut c_void,
) -> RawEventRef {
    // SAFETY: `user` is the `Tap` leaked in `run_tap`, alive while the loop runs.
    let tap = unsafe { &*user.cast::<Tap>() };
    // Borrowed from the system for this call: never dropped here.
    // SAFETY: `event` is a live CGEventRef for the duration of the callback.
    let event = std::mem::ManuallyDrop::new(unsafe { CGEvent::from_ptr(event) });
    if tap.handle(kind, &event) {
        // NULL is how a tap swallows.
        std::ptr::null_mut()
    } else {
        event.as_ptr()
    }
}

/// Runs a CGEventTap on the current thread's run loop. Blocks forever.
pub fn run_tap<F>(on_event: F) -> Result<()>
where
    F: FnMut(Option<RawEvent>) -> bool + 'static,
{
    let tap = Box::into_raw(Box::new(Tap {
        on_event: RefCell::new(Box::new(on_event)),
        port: Cell::new(std::ptr::null()),
    }));

    let mask = [
        KIND_KEY_DOWN,
        KIND_KEY_UP,
        KIND_FLAGS_CHANGED,
        KIND_SYSTEM_DEFINED,
    ]
    .into_iter()
    .fold(0u64, |mask, kind| mask | 1 << kind);
    // SAFETY: `trampoline` matches CGEventTapCallBack and `tap` outlives the
    // tap. Session / head-insert / Default: Default (not ListenOnly) is what
    // lets a callback swallow events.
    let port = unsafe { CGEventTapCreate(1, 0, 0, mask, trampoline, tap.cast()) };
    if port.is_null() {
        // SAFETY: the tap never started, so nothing else holds `tap`.
        drop(unsafe { Box::from_raw(tap) });
        return Err(anyhow!(
            "failed to create event tap — is Accessibility permission granted?"
        ));
    }
    // SAFETY: CGEventTapCreate returns a +1 CFMachPort.
    let mach_port = unsafe { CFMachPort::wrap_under_create_rule(port.cast()) };
    // SAFETY: `tap` is live, and this thread is the only one that reads `port`.
    unsafe { (*tap).port.set(port) };

    let loop_source = mach_port
        .create_runloop_source(0)
        .map_err(|_| anyhow!("failed to create run loop source"))?;

    // Seeded so the first flagsChanged after launch is an edge only if Caps
    // Lock really moved.
    // SAFETY: plain query, no pointers.
    aux::seed_caps(unsafe { CGEventSourceFlagsState(0) } & FLAG_ALPHA_SHIFT != 0);

    let run_loop = CFRunLoop::get_current();
    unsafe { run_loop.add_source(&loop_source, kCFRunLoopCommonModes) };
    unsafe { CGEventTapEnable(port, true) };

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
