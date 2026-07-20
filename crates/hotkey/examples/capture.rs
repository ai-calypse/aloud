//! Live binding-capture check: records three real gestures and prints what
//! each one resolved to. Requires Accessibility permission on macOS.
//!
//! `cargo run -p aloud-hotkey --example capture`
//!
//! This is the half unit tests cannot reach. `capture::Recorder` is pure and
//! tested, but whether the OS backend reports *Right* Option rather than a
//! side-agnostic Option, whether Fn arrives at all, and whether the keys are
//! swallowed instead of reaching the terminal are all facts about the tap.
//! Type into this and the answers are visible.

use aloud_hotkey::model::Config;
use std::sync::{atomic::AtomicBool, mpsc, Arc, Mutex};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    if !aloud_hotkey::is_permitted() {
        eprintln!("Accessibility permission not granted.");
        eprintln!("System Settings > Privacy & Security > Accessibility, then re-run.");
        std::process::exit(1);
    }

    let capture: aloud_hotkey::CaptureSlot = Default::default();
    let slot = capture.clone();

    std::thread::spawn(move || {
        let prompts = [
            "a modifier on its own (try Right Option)",
            "a chord (try Cmd+Shift+D)",
            "anything you would actually bind",
        ];
        for (i, prompt) in prompts.iter().enumerate() {
            let (tx, rx) = mpsc::channel();
            *slot.lock().unwrap() = Some(aloud_hotkey::Capture { id: 0, tx });
            println!("\n[{}/3] Press {prompt}…", i + 1);

            match rx.recv() {
                Ok(binding) => {
                    println!("  captured: {binding:?}");
                    // The listener disarms itself on commit; if this is ever
                    // Some, capture has leaked and the keyboard stays swallowed.
                    assert!(
                        slot.lock().unwrap().is_none(),
                        "listener must disarm capture once a gesture commits"
                    );
                }
                Err(e) => println!("  capture ended: {e}"),
            }
        }
        println!("\nAll three captured, and capture disarmed itself each time.");
        println!("Nothing above should have been typed into this terminal.");
        std::process::exit(0);
    });

    aloud_hotkey::listen(
        Arc::new(Mutex::new(Config::defaults())),
        Arc::new(AtomicBool::new(false)),
        capture,
        |event, _held| println!("  (matcher saw {event:?} — should not happen while capturing)"),
    )
}
