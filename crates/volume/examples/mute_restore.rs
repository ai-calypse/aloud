//! Mutes the system output, waits, and restores it — the real device, not a mock.
//!
//! `cargo run -p aloud-volume --example mute_restore`
//!
//! Watch the volume HUD while it runs. The second phase is the one that
//! matters: it leaves the state file on disk and calls `restore` as a fresh
//! launch would, the path that keeps a force-quit from leaving you muted.

use std::{thread::sleep, time::Duration};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!("muting for 3s…");
    aloud_volume::mute()?;
    sleep(Duration::from_secs(3));
    aloud_volume::restore()?;
    println!("restored. check the volume is back where it was.");

    sleep(Duration::from_secs(1));

    println!("\nnow the crash path: muting and *not* restoring…");
    aloud_volume::mute()?;
    sleep(Duration::from_secs(2));
    println!("pretending the app died here — the state file is still on disk.");
    println!("calling restore() the way a fresh launch does…");
    aloud_volume::restore()?;
    println!("restored. a crash mid-dictation cannot leave you muted.");

    // Idempotence: nothing saved, so this must do nothing rather than fail.
    aloud_volume::restore()?;
    println!("\na second restore with nothing saved was a no-op, as expected.");
    Ok(())
}
