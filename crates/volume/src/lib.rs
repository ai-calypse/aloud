//! Muting the system output around a dictation, and putting it back.
//!
//! Music playing into the microphone is the problem this solves: whisper
//! transcribes it along with the speech. Muting for the length of the capture
//! is the fix, and restoring afterwards is the part that has to be reliable.
//!
//! **The saved volume goes to disk, not just memory.** A crash or a force-quit
//! mid-dictation would otherwise leave the machine silent with no indication
//! why, and nothing in the app to undo it. [`restore`] is therefore safe to
//! call at launch: it is the same operation as the post-dictation restore, and
//! a no-op when there is nothing saved.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Where the pre-mute volume is parked. Deliberately outside the settings
/// file: this is transient recovery state, not a preference, and it must be
/// removed once consumed.
fn state_path() -> Result<PathBuf> {
    Ok(aloud_notes::data_dir()?.join("volume-restore"))
}

/// Silences the output, remembering the level to put back.
///
/// A second call while already muted keeps the *original* level: overwriting
/// it with the current 0 would make the restore a no-op and strand the user
/// at silent, which is exactly the failure the disk file exists to prevent.
pub fn mute() -> Result<()> {
    let path = state_path()?;
    if path.exists() {
        return Ok(());
    }
    let Some(current) = output_volume()? else {
        return Ok(()); // no controllable output on this platform
    };

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    // Written *before* muting: a crash between the two leaves a restorable
    // file and an unmuted machine, which is recoverable. The other order
    // leaves a muted machine and no file, which is not.
    std::fs::write(&path, current.to_string())
        .with_context(|| format!("writing {}", path.display()))?;
    set_output_volume(0)
}

/// Puts the saved volume back and forgets it. A no-op when nothing is saved,
/// so it is safe to call on every stop path and once at launch.
pub fn restore() -> Result<()> {
    let path = state_path()?;
    let Ok(saved) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let result = match saved.trim().parse::<u8>() {
        Ok(level) => set_output_volume(level),
        // A corrupt file must still clear, or it would pin the volume every
        // launch. Losing the level is better than looping on it.
        Err(e) => Err(anyhow::anyhow!("unreadable saved volume {saved:?}: {e}")),
    };
    let _ = std::fs::remove_file(&path);
    result
}

/// The current output volume as 0–100, or None where there is nothing to read.
#[cfg(target_os = "macos")]
fn output_volume() -> Result<Option<u8>> {
    // osascript rather than CoreAudio: this is two shell-outs per dictation,
    // off the hot path, against a stable scripting interface. coreaudio-sys
    // would be a dependency and an unsafe block to save nothing measurable.
    let out = std::process::Command::new("osascript")
        .args(["-e", "output volume of (get volume settings)"])
        .output()
        .context("running osascript to read the volume")?;
    if !out.status.success() {
        anyhow::bail!("osascript could not read the output volume");
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let text = text.trim();
    // "missing value" when the output device has no software volume control,
    // e.g. some USB interfaces and HDMI. Nothing to mute, so nothing to save.
    if text == "missing value" {
        return Ok(None);
    }
    Ok(Some(text.parse::<u8>().with_context(|| {
        format!("parsing output volume {text:?}")
    })?))
}

#[cfg(target_os = "macos")]
fn set_output_volume(level: u8) -> Result<()> {
    let status = std::process::Command::new("osascript")
        .args(["-e", &format!("set volume output volume {level}")])
        .status()
        .context("running osascript to set the volume")?;
    anyhow::ensure!(
        status.success(),
        "osascript could not set the output volume"
    );
    Ok(())
}

// ponytail: Windows is a no-op. The real path is IAudioEndpointVolume via the
// `windows` crate, which is a COM dance worth writing only once a Windows user
// exists to test it — and CI cannot run one today.
#[cfg(not(target_os = "macos"))]
fn output_volume() -> Result<Option<u8>> {
    Ok(None)
}

#[cfg(not(target_os = "macos"))]
fn set_output_volume(_level: u8) -> Result<()> {
    Ok(())
}
