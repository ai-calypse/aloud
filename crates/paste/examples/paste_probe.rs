//! Verifies synthetic paste against a real app, in both clipboard modes.
//!
//! `cargo run -p aloud-paste --example paste_probe`, then click into a text
//! field before the countdown ends.
//!
//! Unit tests cannot cover this: the clipboard and the paste chord are OS
//! state, and `preserve_clipboard` is only meaningfully wrong on real hardware.

use anyhow::Result;
use std::{thread::sleep, time::Duration};

const SENTINEL: &str = "aloud-paste-probe-sentinel";

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("Click into a text field — pasting in 4s...");
    sleep(Duration::from_secs(4));

    // Preserve on: the sentinel must survive the paste.
    set_clipboard(SENTINEL)?;
    aloud_paste::paste_text("preserve_clipboard = true", false, true)?;
    let after_preserve = read_clipboard()?;
    assert_eq!(
        after_preserve, SENTINEL,
        "preserve_clipboard=true must put the previous clipboard back"
    );
    println!("✓ preserve on — clipboard still {SENTINEL:?}");

    sleep(Duration::from_secs(1));

    // Preserve off: the dictated text must be what is left behind.
    set_clipboard(SENTINEL)?;
    let dictated = "preserve_clipboard = false";
    aloud_paste::paste_text(dictated, false, false)?;
    let after_replace = read_clipboard()?;
    assert_eq!(
        after_replace, dictated,
        "preserve_clipboard=false must leave the dictated text on the clipboard"
    );
    println!("✓ preserve off — clipboard now {dictated:?}");

    println!("Both modes OK. Two lines should have been pasted above.");
    Ok(())
}

fn set_clipboard(text: &str) -> Result<()> {
    aloud_paste::set_clipboard(text)
}

fn read_clipboard() -> Result<String> {
    Ok(arboard::Clipboard::new()?.get_text()?)
}
