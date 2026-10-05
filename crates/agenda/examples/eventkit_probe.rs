//! EventKit's view of Reminders (read-only): the grant, the read, its time, and
//! whether it agrees with the scripting read.
//!
//!   cargo run -p aloud-agenda --example eventkit_probe          # status only
//!   cargo run -p aloud-agenda --example eventkit_probe -- --ask # may raise the prompt, once
//!
//! Run from a terminal, macOS attributes the grant to the terminal app, not to
//! this binary; the app's own prompt (it carries
//! `NSRemindersFullAccessUsageDescription`) can only be seen in the app itself.

#[cfg(target_os = "macos")]
fn main() {
    use aloud_agenda::eventkit::{self, Access};
    use std::time::{Duration, Instant};

    let ask = std::env::args().any(|a| a == "--ask");
    let mut access = eventkit::access();
    println!("access: {access:?}");
    if access == Access::Undecided && ask {
        let t = Instant::now();
        println!(
            "asked, granted: {} in {:?}",
            eventkit::request_access(Duration::from_secs(90)),
            t.elapsed()
        );
        access = eventkit::access();
        println!("access now: {access:?}");
    }
    if access != Access::Granted {
        return;
    }
    let to = aloud_agenda::day_start(1);
    for i in 0..3 {
        let t = Instant::now();
        let got = eventkit::reminders(to);
        println!(
            "eventkit read #{i}: {:?} -> {:?} reminders",
            t.elapsed(),
            got.as_ref().map(Vec::len)
        );
    }
    let ek = eventkit::reminders(to).unwrap_or_default();
    let t = Instant::now();
    let js = aloud_agenda::reminders_scripted(to).unwrap_or_default();
    println!("scripted read: {:?} -> {} reminders", t.elapsed(), js.len());
    println!("identical to the scripted read: {}", ek == js);
    if ek != js {
        println!("eventkit: {ek:#?}\nscripted: {js:#?}");
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {}
