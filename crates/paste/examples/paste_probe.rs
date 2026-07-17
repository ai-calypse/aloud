//! Verifies synthetic paste against a real app.
//!
//! `cargo run -p aloud-paste --example paste_probe`, then click into a text
//! field before the countdown ends.

use std::{thread::sleep, time::Duration};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!("Click into a text field — pasting in 4s...");
    sleep(Duration::from_secs(4));

    aloud_paste::paste_text("aloud paste probe OK", false)?;
    println!("Pasted. Previous clipboard should be restored.");
    Ok(())
}
