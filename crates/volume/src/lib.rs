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
//!
//! **Callers on the hotkey thread must use [`mute_soon`] / [`restore_soon`].**
//! Setting the volume shells out to `osascript`, which costs 100ms or more —
//! far too long for the macOS event tap, which stalls every keystroke on the
//! machine while a callback runs and gets disabled outright if one overruns.

use anyhow::{Context, Result};
use std::{
    path::{Path, PathBuf},
    sync::{mpsc, OnceLock},
};

/// Where the pre-mute volume is parked. Deliberately outside the settings
/// file: this is transient recovery state, not a preference, and it must be
/// removed once consumed.
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

/// Silences the output, saving the level to `path`.
///
/// A second call while already muted keeps the *original* level: overwriting
/// it with the current 0 would make the restore a no-op and strand the user
/// at silent, which is exactly the failure the disk file exists to prevent.
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
    // Written *before* muting: a crash between the two leaves a restorable
    // file and an unmuted machine, which is recoverable. The other order
    // leaves a muted machine and no file, which is not.
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
        // A corrupt file must still clear, or it would pin the volume every
        // launch. Losing the level is better than looping on it.
        Err(e) => Err(anyhow::anyhow!("unreadable saved volume {saved:?}: {e}")),
    };
    let _ = std::fs::remove_file(path);
    result
}

/// What the worker below performs, in the order it was asked.
enum Command {
    Mute,
    Restore,
}

/// Queues a mute. Returns immediately.
pub fn mute_soon() {
    send(Command::Mute);
}

/// Queues a restore. Returns immediately.
pub fn restore_soon() {
    send(Command::Restore);
}

fn send(command: Command) {
    let _ = worker().send(command);
}

/// One long-lived thread draining a queue, rather than a task per call.
///
/// Order is the whole point: a dictation stopped moments after it started
/// would otherwise let the restore run before the mute it is undoing, find no
/// saved level, and leave the machine silent for good. A FIFO queue on a
/// single thread makes that unrepresentable — the alternative, spawning both,
/// makes it a race.
fn worker() -> &'static mpsc::Sender<Command> {
    static WORKER: OnceLock<mpsc::Sender<Command>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Command>();
        std::thread::Builder::new()
            .name("aloud-volume".into())
            .spawn(move || {
                for command in rx {
                    let result = match command {
                        Command::Mute => mute(),
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
    // osascript rather than CoreAudio: two shell-outs per dictation, off the
    // hotkey thread via the worker above, against a stable scripting
    // interface. coreaudio-sys would be a dependency and an unsafe block to
    // save nothing the user can perceive.
    //
    // Read and set stay separate processes deliberately. One combined script
    // could halve the spawns, but the saved level has to reach disk *between*
    // the read and the mute, and that ordering is what makes a crash
    // recoverable.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The failure: muting twice would save the already-muted 0 over the real
    /// level, and the restore would then put the user back to silent — the
    /// exact state the saved file exists to prevent.
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

    /// The failure: a corrupt file that is read but never removed would be
    /// re-read on every launch, pinning the volume forever. Losing the level
    /// once is recoverable; looping on it is not.
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

    /// Restore is called on every stop path and once at launch, so the
    /// nothing-saved case has to be silent rather than an error.
    #[test]
    fn restoring_with_nothing_saved_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("volume-restore");

        assert!(restore_at(&path).is_ok());
        assert!(!path.exists());
    }
}

// Note on coverage: every test above exercises a branch that returns *before*
// `set_output_volume`. That is deliberate, not an oversight — on macOS these
// functions drive the real output device, so a test of the successful restore
// path would change the volume of whatever machine ran `cargo test` and leave
// it there. The happy path is covered by `examples/mute_restore`, against real
// hardware, where the side effect is the point.
