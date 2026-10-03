//! Drives a real app through the screen tools, as a command would.
//!
//! `cargo run -p aloud-screen --example screen` (macOS, Cua Driver running).
//! Opens a new TextEdit document, reads it, types into its text area by element
//! number, clicks Bold, reads the document back, and closes it unsaved.

use anyhow::{bail, ensure, Context, Result};
use serde_json::json;
use std::process::Command;
use std::time::Instant;

fn main() -> Result<()> {
    ensure!(
        !aloud_screen::tools().is_empty(),
        "Cua Driver is not installed"
    );
    osascript(r#"tell application "TextEdit" to make new document"#)?;
    std::thread::sleep(std::time::Duration::from_millis(800));

    let run = || -> Result<()> {
        let started = Instant::now();
        let read = step(aloud_screen::READ, json!({ "app": "TextEdit" }))?;
        println!("read in {:?}", started.elapsed());
        let tree = read["elements"].as_str().context("no tree")?;
        let area = element(tree, "AXTextArea")?;
        let bold = element(tree, "(bold)")?;

        let started = Instant::now();
        let typed = step(
            aloud_screen::TYPE,
            json!({ "app": "TextEdit", "element": area, "text": "typed by aloud-screen" }),
        )?;
        println!(
            "type + re-read in {:?}: {}",
            started.elapsed(),
            typed["done"]
        );

        // Numbers come from the tree the action returned, as the model's would.
        let bold = element(
            typed["now"]["elements"].as_str().unwrap_or_default(),
            "(bold)",
        )
        .unwrap_or(bold);
        let started = Instant::now();
        let clicked = step(
            aloud_screen::CLICK,
            json!({ "app": "TextEdit", "element": bold }),
        )?;
        println!(
            "click + re-read in {:?}: {}",
            started.elapsed(),
            clicked["done"]
        );

        let text = osascript(r#"tell application "TextEdit" to get text of document 1"#)?;
        ensure!(
            text.trim() == "typed by aloud-screen",
            "document holds {text:?}"
        );
        println!("✓ the text landed in the document");
        Ok(())
    };
    let result = run();
    aloud_screen::finish();
    osascript(r#"tell application "TextEdit" to close document 1 saving no"#)?;
    result
}

fn step(tool: &str, args: serde_json::Value) -> Result<serde_json::Value> {
    let out = aloud_screen::execute(tool, &args).context("not a screen tool")?;
    if let Some(error) = out["error"].as_str() {
        bail!("{tool} failed: {error}");
    }
    Ok(out)
}

/// The `[N]` on the first tree line containing `needle`.
fn element(tree: &str, needle: &str) -> Result<u64> {
    let line = tree
        .lines()
        .find(|l| l.contains(needle))
        .with_context(|| format!("no {needle} in the tree"))?;
    let number = line.split('[').nth(1).and_then(|s| s.split(']').next());
    number
        .and_then(|n| n.parse().ok())
        .with_context(|| format!("no element number on {line:?}"))
}

fn osascript(script: &str) -> Result<String> {
    let out = Command::new("osascript").args(["-e", script]).output()?;
    ensure!(
        out.status.success(),
        "osascript: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
