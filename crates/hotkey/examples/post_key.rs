//! Posts real volume-key, brightness-key and Caps Lock events for
//! `--example keys` to see.
//!
//!   cargo run -p aloud-hotkey --example post_key -- volume up|down|mute [option-shift]
//!   LOC=1 cargo run -p aloud-hotkey --example post_key -- brightness up|down [option-shift|keycode]
//!   cargo run -p aloud-hotkey --example post_key -- caps on|off
//!
//! A volume key is an NX_SYSDEFINED event built through `NSEvent`, posted at
//! the HID level so macOS acts on it as well unless a tap eats it: it moves
//! the real volume. Brightness is the same event with key type 2/3, or with
//! `keycode` a plain key-down of keycode 144/145; post it with `LOC=1`
//! (session level, after the HID system has acted) so it exercises the tap
//! without macOS moving the real brightness. Caps Lock is a flagsChanged with keycode 57, which only
//! the tap hears; the keyboard's real Caps Lock state is not touched.

#[cfg(target_os = "macos")]
fn main() {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send, Encode, Encoding};
    use std::{thread::sleep, time::Duration};

    #[repr(C)]
    struct Point {
        x: f64,
        y: f64,
    }
    // SAFETY: NSPoint is two CGFloats.
    unsafe impl Encode for Point {
        const ENCODING: Encoding =
            Encoding::Struct("CGPoint", &[Encoding::Double, Encoding::Double]);
    }
    /// `CGEventRef` as Objective-C spells it.
    #[repr(transparent)]
    struct EventRef(*mut std::ffi::c_void);
    // SAFETY: a pointer to the opaque `__CGEvent` struct.
    unsafe impl Encode for EventRef {
        const ENCODING: Encoding = Encoding::Pointer(&Encoding::Struct("__CGEvent", &[]));
    }
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}
    extern "C" {
        fn CGEventPost(tap: u32, event: *mut std::ffi::c_void);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("brightness") if args.iter().any(|a| a == "keycode") => {
            let up = match args.get(1).map(String::as_str) {
                Some("up") => true,
                Some("down") => false,
                _ => panic!("brightness up|down"),
            };
            let code = if up { 144 } else { 145 };
            let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
            for down in [true, false] {
                let e = CGEvent::new_keyboard_event(src.clone(), code, down).unwrap();
                if args.iter().any(|a| a == "option-shift") {
                    e.set_flags(CGEventFlags::CGEventFlagAlternate | CGEventFlags::CGEventFlagShift);
                }
                e.post(CGEventTapLocation::Session);
                sleep(Duration::from_millis(60));
            }
            println!("posted brightness keycode {code}");
        }
        Some(kind @ ("volume" | "brightness")) => {
            let code: i64 = match (kind, args.get(1).map(String::as_str)) {
                ("volume", Some("up")) => 0,
                ("volume", Some("down")) => 1,
                ("volume", Some("mute")) => 7,
                ("brightness", Some("up")) => 2,
                ("brightness", Some("down")) => 3,
                _ => panic!("volume up|down|mute, brightness up|down"),
            };
            // NSEventModifierFlagOption | NSEventModifierFlagShift
            let flags: usize = if args.iter().any(|a| a == "option-shift") {
                (1 << 19) | (1 << 17)
            } else {
                0
            };
            for state in [0xAi64, 0xB] {
                let data1: isize = ((code << 16) | (state << 8)) as isize;
                let event: *mut AnyObject = unsafe {
                    msg_send![class!(NSEvent),
                        otherEventWithType: 14usize,
                        location: Point { x: 0.0, y: 0.0 },
                        modifierFlags: flags,
                        timestamp: 0.0f64,
                        windowNumber: 0isize,
                        context: std::ptr::null_mut::<AnyObject>(),
                        subtype: 8i16,
                        data1: data1,
                        data2: -1isize]
                };
                let cg: EventRef = unsafe { msg_send![event, CGEvent] };
                // 0 HID (the default, as a real key arrives), 1 session, 2 annotated.
                let at = std::env::var("LOC").ok().and_then(|v| v.parse().ok());
                unsafe { CGEventPost(at.unwrap_or(0), cg.0) };
                sleep(Duration::from_millis(60));
            }
            println!("posted {kind} {:?}", &args[1..]);
        }
        Some("caps") => {
            let on = args.get(1).map(String::as_str) == Some("on");
            let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
            let e = CGEvent::new_keyboard_event(src, 57, on).unwrap();
            e.set_type(CGEventType::FlagsChanged);
            e.set_flags(if on {
                CGEventFlags::CGEventFlagAlphaShift
            } else {
                CGEventFlags::empty()
            });
            e.post(CGEventTapLocation::Session);
            println!("posted caps {}", if on { "on" } else { "off" });
        }
        _ => eprintln!("usage: post_key volume up|down|mute [option-shift] | brightness up|down [option-shift|keycode] | caps on|off"),
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("post_key: macOS only");
}
