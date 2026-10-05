//! Prints the watcher's state, then each new notification's app id and title,
//! for N seconds. The title is printed by this local example only; the app
//! never logs it.
//!
//!     cargo run -p aloud-notify --example nudges -- 60
//!
//! Needs Full Disk Access for the terminal running it; without it the state is
//! `needs Full Disk Access` and the example exits at once.

use std::sync::mpsc;
use std::time::Duration;

use aloud_notify::{watch, MirrorState};

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let (tx, rx) = mpsc::channel::<String>();
    let states = tx.clone();
    let _watch = watch(
        move |state| {
            let _ = states.send(format!("state: {state}"));
            if !matches!(state, MirrorState::Watching) {
                let _ = states.send(String::new());
            }
        },
        move |n| {
            let _ = tx.send(format!("{}  {}", n.app, n.title));
        },
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(seconds);
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(line) if line.is_empty() => return,
            Ok(line) => println!("{line}"),
            Err(_) => break,
        }
    }
    println!("done after {seconds}s");
}
