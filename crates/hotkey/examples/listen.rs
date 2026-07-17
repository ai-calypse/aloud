//! Live hotkey check: hold Right Option to see activate/deactivate, Escape to
//! cancel. Requires Accessibility permission on macOS.
//!
//! `cargo run -p aloud-hotkey --example listen`

use aloud_hotkey::model::Config;
use std::sync::{Arc, Mutex};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    if !aloud_hotkey::is_permitted() {
        eprintln!("Accessibility permission not granted.");
        eprintln!("System Settings > Privacy & Security > Accessibility, then re-run.");
        std::process::exit(1);
    }

    println!("Hold Right Option to trigger. Escape cancels. Ctrl+C to quit.\n");

    aloud_hotkey::listen(Arc::new(Mutex::new(Config::defaults())), |event| {
        println!("  {event:?}");
    })
}
