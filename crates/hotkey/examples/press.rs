// Posts a real Right Option hold, as hardware does. Arg: hold duration in ms.
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::{thread::sleep, time::Duration};

fn main() {
    let ms: u64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(2000);
    let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
    let post = |down: bool, flags: CGEventFlags| {
        let e = CGEvent::new_keyboard_event(src.clone(), 61, down).unwrap();
        e.set_type(CGEventType::FlagsChanged);
        e.set_flags(flags);
        e.post(CGEventTapLocation::Session);
    };
    post(true, CGEventFlags::CGEventFlagAlternate);
    println!("holding RightOption for {ms}ms...");
    sleep(Duration::from_millis(ms));
    post(false, CGEventFlags::empty());
    println!("released");
}
