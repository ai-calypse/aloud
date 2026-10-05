//! The notch's Shelf: files parked on it, screenshots, downloads and drives.
//!
//! No Tauri here. The app (`src-tauri/src/shelf.rs`) owns the commands, the
//! islands and the on/off switches; this crate owns what can be wrong quietly
//! — which file name is a screenshot, when a download is really finished,
//! which volume is an external drive — and keeps it under test.

pub mod airdrop;
pub mod downloads;
pub mod drives;
pub mod files;
pub mod prefs;
pub mod shots;
pub mod thumb;
pub mod watch;

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

/// Runs a macOS tool and turns a failure into its own stderr — "Volume is in
/// use", "Error 13: file not found" — which is the sentence worth showing.
pub(crate) fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("could not run {program}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let said = err.trim();
        bail!(
            "{}",
            if said.is_empty() {
                format!("{program} failed")
            } else {
                said.to_string()
            }
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `$HOME`, or nothing: every folder here is the user's own.
pub(crate) fn home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(Into::into)
}

/// Modification time, in whole unix seconds.
pub(crate) fn mtime_secs(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// Now, in whole unix seconds.
pub(crate) fn mtime_secs_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}
