//! Reads the built-in display's brightness and steps it up and down once, as a
//! brightness key would, then puts it back exactly.
//!
//!   cargo run -p aloud-mac --example brightness

fn main() {
    use aloud_mac::brightness;

    println!(
        "controllable (DisplayServices loads, built-in display is main): {}",
        brightness::controllable()
    );
    let before = match brightness::get() {
        Ok(level) => level,
        Err(e) => {
            println!("get: {e:#}");
            return;
        }
    };
    println!("brightness {before:.4} ({}%)", brightness::percent(before));

    // Restored by the guard even if a step panics or errors out.
    struct Restore(f32);
    impl Drop for Restore {
        fn drop(&mut self) {
            match brightness::set(self.0) {
                Ok(()) => println!("restored {:.4}", brightness::get().unwrap_or(f32::NAN)),
                Err(e) => eprintln!("COULD NOT RESTORE {:.4}: {e:#}", self.0),
            }
        }
    }
    let _restore = Restore(before);

    for (label, up, fine) in [
        ("up", true, false),
        ("down", false, false),
        ("fine up", true, true),
        ("fine down", false, true),
    ] {
        match brightness::nudge(up, fine) {
            Ok(now) => println!("{label:>9}: {now:.4} ({}%)", brightness::percent(now)),
            Err(e) => println!("{label:>9}: {e:#}"),
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
}
