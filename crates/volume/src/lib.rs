//! Muting the system output around a dictation, so music does not end up in the
//! transcript, and putting it back.
//!
//! **The saved volume goes to disk**, so a crash mid-dictation does not leave
//! the machine silent with nothing in the app to undo it. [`restore`] is
//! therefore safe to call at launch.
//!
//! **Callers on the hotkey thread must use [`mute_soon`] / [`restore_soon`].**
//! Setting the volume shells out to `osascript` (100ms+), and the macOS event
//! tap stalls every keystroke while a callback runs — and is disabled outright
//! if one overruns.

use anyhow::{Context, Result};
use std::{
    path::{Path, PathBuf},
    sync::{mpsc, OnceLock},
    time::Duration,
};

/// Where the pre-mute volume is parked — outside the settings file, since this
/// is transient recovery state that must be removed once consumed.
fn state_path() -> Result<PathBuf> {
    Ok(aloud_notes::data_dir()?.join("volume-restore"))
}

/// Silences the output, remembering the level to put back. Blocks on
/// `osascript`; see [`mute_soon`] for the hotkey-thread caller.
pub fn mute() -> Result<()> {
    mute_at(&state_path()?)
}

/// Puts the saved volume back and forgets it. A no-op when nothing is saved,
/// so it is safe to call on every stop path and once at launch. Blocks on
/// `osascript`; see [`restore_soon`] for the hotkey-thread caller.
pub fn restore() -> Result<()> {
    restore_at(&state_path()?)
}

/// Silences the output, saving the level to `path`. A second call while already
/// muted keeps the *original* level; overwriting it with the current 0 would
/// strand the user at silent.
fn mute_at(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let Some(current) = output_volume()? else {
        return Ok(()); // no controllable output on this platform
    };

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    // Written *before* muting: a crash between the two leaves an unmuted
    // machine, where the other order leaves a muted one and no file.
    std::fs::write(path, current.to_string())
        .with_context(|| format!("writing {}", path.display()))?;
    set_output_volume(0)
}

/// Restores the level saved in `path` and removes it.
fn restore_at(path: &Path) -> Result<()> {
    let Ok(saved) = std::fs::read_to_string(path) else {
        return Ok(());
    };
    let result = match saved.trim().parse::<u8>() {
        Ok(level) => set_output_volume(level),
        // A corrupt file must still clear, or it pins the volume every launch.
        Err(e) => Err(anyhow::anyhow!("unreadable saved volume {saved:?}: {e}")),
    };
    let _ = std::fs::remove_file(path);
    result
}

/// What the worker below performs, in the order it was asked.
enum Command {
    Mute { lead: Duration },
    Restore,
}

/// Queues a mute, held off for `lead` first: the caller's start chime plays
/// through the output this silences. `Duration::ZERO` when there is no chime.
pub fn mute_soon(lead: Duration) {
    send(Command::Mute { lead });
}

/// Queues a restore. Returns immediately.
pub fn restore_soon() {
    send(Command::Restore);
}

fn send(command: Command) {
    let _ = worker().send(command);
}

/// One long-lived thread draining a FIFO, not a task per call. Order is the
/// point: a dictation stopped moments after starting would otherwise restore
/// before the mute it undoes, find no saved level, and leave the machine silent.
fn worker() -> &'static mpsc::Sender<Command> {
    static WORKER: OnceLock<mpsc::Sender<Command>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Command>();
        std::thread::Builder::new()
            .name("aloud-volume".into())
            .spawn(move || {
                for command in rx {
                    let result = match command {
                        Command::Mute { lead } => {
                            // Sleeping here, not before queueing: a restore
                            // queued behind this must still run after it.
                            std::thread::sleep(lead);
                            mute()
                        }
                        Command::Restore => restore(),
                    };
                    if let Err(e) = result {
                        tracing::warn!(%e, "volume command failed");
                    }
                }
            })
            .expect("spawning the volume worker");
        tx
    })
}

/// The current output volume as 0–100, or None where there is nothing to read.
#[cfg(target_os = "macos")]
fn output_volume() -> Result<Option<u8>> {
    // osascript rather than coreaudio-sys: a dependency and an unsafe block to
    // save nothing perceptible. Read and set stay separate processes — the saved
    // level has to reach disk *between* them for a crash to be recoverable.
    let out = std::process::Command::new("osascript")
        .args(["-e", "output volume of (get volume settings)"])
        .output()
        .context("running osascript to read the volume")?;
    if !out.status.success() {
        anyhow::bail!("osascript could not read the output volume");
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let text = text.trim();
    // "missing value" for a device with no software volume control (some USB
    // interfaces, HDMI). Nothing to mute, so nothing to save.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards: muting twice saved the already-muted 0 over the real level, so
    /// the restore put the user back to silent.
    #[test]
    fn muting_twice_keeps_the_first_saved_level() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("volume-restore");
        std::fs::write(&path, "42").unwrap();

        mute_at(&path).unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "42",
            "the second mute must not overwrite the saved level"
        );
    }

    /// Guards: a corrupt file that is read but never removed pins the volume on
    /// every launch.
    #[test]
    fn a_corrupt_saved_level_is_reported_and_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("volume-restore");
        std::fs::write(&path, "not a number").unwrap();

        assert!(
            restore_at(&path).is_err(),
            "the caller should hear about it"
        );
        assert!(!path.exists(), "a corrupt file must still be cleared");
    }

    /// Restore runs on every stop path and at launch, so nothing-saved must be
    /// silent rather than an error.
    #[test]
    fn restoring_with_nothing_saved_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("volume-restore");

        assert!(restore_at(&path).is_ok());
        assert!(!path.exists());
    }
}

// Every test above returns *before* `set_output_volume`, deliberately: these
// drive the real output device, so testing the happy path would change the
// volume of whatever machine ran `cargo test`. `examples/mute_restore` covers it.
