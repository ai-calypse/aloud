//! Prints the notch's system events as they happen, so each can be checked on
//! the real Mac. Run: `cargo run -p aloud-mac --example mac_events -- [power|display|network|all] [seconds]`
//!
//! Prints what each source reads at rest first, then every event it would
//! whisper, with the island it would build. Plug or unplug the charger, toggle
//! Low Power Mode, connect a display, switch Wi-Fi or a VPN while it runs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use aloud_mac::events::{self, Event, Sources};

#[cfg(target_os = "macos")]
fn main() {
    let mut args = std::env::args().skip(1);
    let which = args.next().unwrap_or_else(|| "all".into());
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);
    let sources = Sources {
        power: matches!(which.as_str(), "power" | "all"),
        display: matches!(which.as_str(), "display" | "all"),
        network: matches!(which.as_str(), "network" | "all"),
    };

    for line in events::snapshot() {
        println!("at rest  {line}");
    }
    let start = Instant::now();
    events::start(
        sources,
        Arc::new(move |e: Event| {
            let at = start.elapsed().as_secs_f32();
            match e.island() {
                Some(i) => println!(
                    "{at:6.1}s  {e:?}  ->  {} [{}] \"{}\" short \"{}\"",
                    i.id,
                    i.icon.unwrap_or_default(),
                    i.title,
                    i.short.unwrap_or_default()
                ),
                None => println!("{at:6.1}s  {e:?}"),
            }
        }),
    );
    println!("watching {which} for {secs}s");
    // The main thread owns the run loop every source was installed on.
    while start.elapsed() < Duration::from_secs(secs) {
        core_foundation::runloop::CFRunLoop::run_in_mode(
            // SAFETY: a static constant.
            unsafe { core_foundation::runloop::kCFRunLoopDefaultMode },
            Duration::from_millis(250),
            false,
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("mac_events is macOS only");
}
