//! Alert chimes for recording start / stop / error.
//!
//! Shells out to each OS's own sound facility rather than bundling audio and a
//! mixer (rodio) for three UI chimes; Linux gets silence. Fire-and-forget — a
//! chime must never block or fail the dictation pipeline, so errors are logged
//! and swallowed.

#[derive(Debug, Clone, Copy)]
pub enum Chime {
    Start,
    Stop,
    Error,
}

/// Plays `chime` if `enabled`, at `volume` (0.0–1.0). Spawns a detached
/// process and returns at once; the pipeline never waits on audio.
pub fn play(chime: Chime, enabled: bool, volume: f32) {
    if !enabled {
        return;
    }
    if let Err(e) = play_impl(chime, volume.clamp(0.0, 1.0)) {
        tracing::debug!(%e, ?chime, "could not play alert sound");
    }
}

#[cfg(target_os = "macos")]
fn play_impl(chime: Chime, volume: f32) -> std::io::Result<()> {
    // These files ship with every macOS.
    let name = match chime {
        Chime::Start => "Tink",
        Chime::Stop => "Pop",
        Chime::Error => "Basso",
    };
    std::process::Command::new("afplay")
        .arg("-v")
        .arg(volume.to_string())
        .arg(format!("/System/Library/Sounds/{name}.aiff"))
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn play_impl(chime: Chime, _volume: f32) -> std::io::Result<()> {
    // SystemSounds has no distinct "start" tone and no per-sound volume, so the
    // volume setting does not apply here.
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
fn play_impl(_chime: Chime, _volume: f32) -> std::io::Result<()> {
    Ok(())
}
