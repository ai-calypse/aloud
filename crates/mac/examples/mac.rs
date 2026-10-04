//! Live check of everything in this crate against the real Mac — the part
//! unit tests cannot prove. Run: `cargo run -p aloud-mac --example mac -- <stats|power|privacy|windows [left|right|max|tl]>`
//!
//! `privacy` polls every second until interrupted, so it can be run beside
//! a call or a camera app. `windows` only reads until given a position, then snaps the window that
//! was in front.

use aloud_mac::{power, privacy, stats, windows};

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("stats") => {
            let mut sampler = stats::Sampler::new();
            for _ in 0..3 {
                println!("{:#?}", sampler.sample());
            }
        }
        Some("power") => println!("{:#?}", power::read()),
        Some("privacy") => loop {
            println!("{:?}", privacy::read());
            std::thread::sleep(std::time::Duration::from_secs(1));
        },
        Some("windows") => {
            println!("trusted: {}", windows::is_trusted());
            println!("front: {:?}", windows::front_window());
            // NSScreen wants the main thread, which `main` is.
            let screens = windows::visible_screens();
            println!("screens: {screens:?}");
            // Moving someone's window is opt-in: name a position to do it.
            let snap = match args.next().as_deref() {
                Some("left") => windows::Snap::Left,
                Some("right") => windows::Snap::Right,
                Some("max") => windows::Snap::Maximize,
                Some("tl") => windows::Snap::TopLeft,
                _ => return,
            };
            println!("{:?}", windows::snap(snap, &screens));
        }
        _ => eprintln!("usage: mac <stats|power|privacy|windows [left|right|max|tl]>"),
    }
}
