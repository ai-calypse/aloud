//! Regression check for the off-main-thread SIGTRAP: enigo's `Key::Unicode`
//! lookup calls TIS/TSM, which macOS aborts on (`dispatch_assert_queue`) off the
//! main queue. Calls `copy_selection()` from a background thread the way the
//! pipeline does — before the fix this killed the process.
//!
//! Harmless to run: it synthesizes Cmd+C and restores the clipboard.

fn main() {
    // Arm TSM on the main thread first: the assert only fires once TSM has a
    // home queue.
    match aloud_paste::copy_selection() {
        Ok(text) => println!("main thread: OK, {} chars", text.len()),
        Err(e) => println!("main thread: ERR {e:#}"),
    }

    std::thread::spawn(|| {
        match aloud_paste::copy_selection() {
            Ok(text) => println!("OK: copy_selection returned {} chars", text.len()),
            Err(e) => println!("ERR (no crash, still a pass): {e:#}"),
        }
        std::process::exit(0);
    });

    #[cfg(target_os = "macos")]
    {
        // Stand in for the app's run loop so the main queue is serviced.
        extern "C" {
            fn dispatch_main() -> !;
        }
        unsafe { dispatch_main() }
    }
    #[cfg(not(target_os = "macos"))]
    std::thread::sleep(std::time::Duration::from_secs(5));
}
