// Posts a real modifier hold, as hardware does.
// Args: hold duration in ms, then an optional macOS keycode (default 61,
// Right Option; 58 is Left Option, which command mode binds by default).
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
    let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
    let post = |down: bool, flags: CGEventFlags| {
        let e = CGEvent::new_keyboard_event(src.clone(), keycode, down).unwrap();
        e.set_type(CGEventType::FlagsChanged);
        e.set_flags(flags);
        e.post(CGEventTapLocation::Session);
    };
    post(true, CGEventFlags::CGEventFlagAlternate);
    println!("holding keycode {keycode} for {ms}ms...");
    sleep(Duration::from_millis(ms));
    post(false, CGEventFlags::empty());
    println!("released");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("press: macOS only (synthesizes a CGEvent)");
}
