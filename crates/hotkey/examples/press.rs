// Posts a real modifier hold, as hardware does.
// Args: hold duration in ms, then an optional macOS keycode (default 61,
// Right Option; 58 is Left Option, which rewrite binds by default).
//
// macOS only: it synthesizes a CGEvent, the one tool that actually drives the
// hotkey path (osascript-synthesized keys arrive with keycode 0 — see
// CLAUDE.md). Gated so the crate still builds under `--all-targets` on Windows.

#[cfg(target_os = "macos")]
fn main() {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use std::{thread::sleep, time::Duration};

    let ms: u64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(2000);
    let keycode: u16 = std::env::args()
        .nth(2)
        .and_then(|a| a.parse().ok())
        .unwrap_or(61);
    // Optional third arg: a second modifier keycode held *around* the binding,
    // for testing that a chord does or does not fire. 55 is Left Command.
    let extra: Option<u16> = std::env::args().nth(3).and_then(|a| a.parse().ok());

    // Real hardware sets both the generic mask (CGEventFlagAlternate) and the
    // side-specific NX_DEVICE* bit. The backend reads the side-specific one —
    // the generic bit cannot tell Left Shift from Right Shift — so an event
    // carrying only the generic bit reads as "released" and drives nothing.
    // Emulating just the generic mask made this example silently useless.
    let device_bit = |code: u16| -> u64 {
        match code {
            59 => 0x0000_0001, // Left Control
            56 => 0x0000_0002, // Left Shift
            60 => 0x0000_0004, // Right Shift
            55 => 0x0000_0008, // Left Command
            54 => 0x0000_0010, // Right Command
            58 => 0x0000_0020, // Left Option
            61 => 0x0000_0040, // Right Option
            62 => 0x0000_2000, // Right Control
            _ => 0,
        }
    };

    let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
    let post = |code: u16, down: bool, bits: u64| {
        let e = CGEvent::new_keyboard_event(src.clone(), code, down).unwrap();
        e.set_type(CGEventType::FlagsChanged);
        // retain, not truncate: the NX_DEVICE* bits are not declared in
        // CGEventFlags, and truncating silently drops exactly the bits the
        // backend reads — which is why this example fired nothing at all.
        e.set_flags(CGEventFlags::from_bits_retain(bits));
        e.post(CGEventTapLocation::Session);
    };

    // Flags are cumulative and describe the whole keyboard, not the key that
    // moved: every event must carry the bits of everything still held, or the
    // tap reads the others as released.
    let mut held: u64 = 0;

    if let Some(code) = extra {
        held |= CGEventFlags::CGEventFlagCommand.bits() | device_bit(code);
        post(code, true, held);
        println!("holding extra keycode {code}");
    }

    held |= CGEventFlags::CGEventFlagAlternate.bits() | device_bit(keycode);
    post(keycode, true, held);
    println!("holding keycode {keycode} for {ms}ms...");
    sleep(Duration::from_millis(ms));

    held &= !(CGEventFlags::CGEventFlagAlternate.bits() | device_bit(keycode));
    post(keycode, false, held);
    if let Some(code) = extra {
        post(code, false, 0);
    }
    println!("released");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("press: macOS only (synthesizes a CGEvent)");
}
