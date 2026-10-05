//! Connected Bluetooth devices with their batteries, then every connect and
//! disconnect for N seconds, with the notch takeover each would raise.
//!
//!     cargo run -p aloud-media --example bluetooth -- 60
//!
//! To see an edge, take AirPods out of the case (or put them back), or switch a
//! Bluetooth speaker off and on. This example never touches Bluetooth itself.

#[cfg(target_os = "macos")]
fn main() {
    use std::time::Duration;

    use aloud_media::devices::{check_batteries, list, react, watch};

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let devices = list().unwrap_or_default();
    println!("paired devices ({}):", devices.len());
    for d in &devices {
        let parts: Vec<String> = d
            .batteries
            .iter()
            .map(|b| format!("{} {}%", b.label, b.percent).trim().to_string())
            .collect();
        println!(
            "  {} {} [{}] {} {}",
            if d.connected { "on " } else { "off" },
            d.name,
            d.kind,
            d.address,
            parts.join(", ")
        );
    }
    println!("low-battery peeks now: {:#?}", check_batteries(&devices));

    println!("watching for {secs}s ...");
    watch(|event| {
        println!("[edge] {event:?}");
        // The app does the same on a blocking thread; `react` waits for the
        // batteries on a connect, so it must not run on the run loop.
        std::thread::spawn(move || {
            for island in react(event) {
                println!(
                    "  -> {:?} {:?} id={} icon={:?} short={:?} title={:?}",
                    island.tier, island.source, island.id, island.icon, island.short, island.title
                );
            }
        });
    });
    // IOBluetooth delivers on the main run loop: pump it, as the app's does.
    let result = core_foundation::runloop::CFRunLoop::run_in_mode(
        // SAFETY: a CoreFoundation constant.
        unsafe { core_foundation::runloop::kCFRunLoopDefaultMode },
        Duration::from_secs(secs),
        false,
    );
    println!("run loop: {result:?}");
    println!("done");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("macOS only");
}
