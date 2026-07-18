//! Alert chimes for recording start / stop / error.
//!
//! voice-dict played named macOS system sounds through `NSSound` (Tink on
//! start, Pop on stop, Basso on error). There is no cross-platform `NSSound`,
//! and bundling audio + a mixer (rodio) is overkill for three UI chimes — so
//! this uses each OS's own sound facility, behind `#[cfg]`, per the CLAUDE.md
//! "platform differences live behind cfg, use the better Rust tool" guidance:
//!
//! - macOS: `afplay /System/Library/Sounds/<Name>.aiff` — the exact sound set
//!   voice-dict used, no dependency, no bundled files.
//! - Windows: PowerShell `[System.Media.SystemSounds]` — the native equivalents.
//! - Otherwise: silent (no assumption about a Linux sound daemon).
//!
//! Fire-and-forget: a chime must never block or fail the dictation pipeline, so
//! errors are logged and swallowed.

/// The three moments that get a sound, mapped to each OS's nearest equivalent.
#[derive(Debug, Clone, Copy)]
pub enum Chime {
    /// Recording started.
    Start,
    /// Recording stopped / transcription begins.
    Stop,
    /// Something failed.
    Error,
}

/// Plays `chime` if `enabled`. Spawns a detached process and returns at once;
/// the pipeline never waits on audio.
pub fn play(chime: Chime, enabled: bool) {
    if !enabled {
        return;
    }
    if let Err(e) = play_impl(chime) {
        tracing::debug!(%e, ?chime, "could not play alert sound");
    }
}

#[cfg(target_os = "macos")]
fn play_impl(chime: Chime) -> std::io::Result<()> {
    // The names voice-dict used; these files ship with every macOS.
    let name = match chime {
        Chime::Start => "Tink",
        Chime::Stop => "Pop",
        Chime::Error => "Basso",
    };
    std::process::Command::new("afplay")
        .arg(format!("/System/Library/Sounds/{name}.aiff"))
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn play_impl(chime: Chime) -> std::io::Result<()> {
    // SystemSounds has no distinct "start" tone; Asterisk/Hand are the nearest
    // informational/error pair, with Beep standing in for the stop tick.
    let sound = match chime {
        Chime::Start => "Asterisk",
        Chime::Stop => "Beep",
        Chime::Error => "Hand",
    };
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("[System.Media.SystemSounds]::{sound}.Play()"),
        ])
        .spawn()
        .map(|_| ())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn play_impl(_chime: Chime) -> std::io::Result<()> {
    Ok(())
}
