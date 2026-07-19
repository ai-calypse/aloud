//! Live hotkey check: hold Right Option to see activate/deactivate, Escape to
//! cancel. Requires Accessibility permission on macOS.
//!
//! `cargo run -p aloud-hotkey --example listen`

use aloud_hotkey::model::Config;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    if !aloud_hotkey::is_permitted() {
        eprintln!("Accessibility permission not granted.");
        eprintln!("System Settings > Privacy & Security > Accessibility, then re-run.");
        std::process::exit(1);
    }

    println!("Hold Right Option to trigger. Escape cancels. Ctrl+C to quit.");
    println!("Releasing the key starts a fake 10s 'transcribing' window —");
    println!("press Escape inside it to prove Escape survives the key release.\n");

    // Stands in for the pipeline's busy flag: set while a session is still
    // running after its key was released, which is the window the real bug
    // left uncancellable.
    let busy = Arc::new(AtomicBool::new(false));
    let flag = busy.clone();

    aloud_hotkey::listen(
        Arc::new(Mutex::new(Config::defaults())),
        busy,
        move |event| {
            use aloud_hotkey::model::Event;
            println!("  {event:?}");

            match event {
                Event::Deactivated(_) => {
                    flag.store(true, Ordering::Relaxed);
                    println!("    [busy] transcribing… Escape should cancel for 10s");
                    let flag = flag.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(10));
                        if flag.swap(false, Ordering::Relaxed) {
                            println!("    [busy] window closed on its own");
                        }
                    });
                }
                Event::Cancelled => {
                    flag.store(false, Ordering::Relaxed);
                    println!("    [busy] cancelled");
                }
                _ => {}
            }
        },
    )
}
