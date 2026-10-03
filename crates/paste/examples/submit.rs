//! Verifies a paste followed by the spoken "press enter" Return, in TextEdit.
//!
//! `cargo run -p aloud-paste --example submit` (macOS). Opens a new TextEdit
//! document, pastes one line with Return and one without, reads the document
//! back, and closes it unsaved. The Return lands in another app's event queue
//! behind the paste, so only real hardware can say it arrives after the text.

use anyhow::{ensure, Context, Result};
use std::process::Command;
use std::{thread::sleep, time::Duration};

fn main() -> Result<()> {
    osascript(
        r#"tell application "TextEdit" to activate
tell application "TextEdit" to make new document"#,
    )?;
    sleep(Duration::from_millis(800));

    aloud_paste::paste_text("first line", true, true)?;
    aloud_paste::paste_text("second line", false, true)?;
    sleep(Duration::from_millis(300));

    let text = osascript(r#"tell application "TextEdit" to get text of document 1"#)?;
    osascript(r#"tell application "TextEdit" to close document 1 saving no"#)?;

    ensure!(
        text.trim_end() == "first line\nsecond line",
        "expected the Return between the two pastes, got {text:?}"
    );
    println!("✓ Return arrived after the pasted text");
    Ok(())
}

fn osascript(script: &str) -> Result<String> {
    let out = Command::new("osascript")
        .args(["-e", script])
        .output()
        .context("running osascript")?;
    ensure!(
        out.status.success(),
        "osascript failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8_lossy(&out.stdout).replace('\r', "\n"))
}
