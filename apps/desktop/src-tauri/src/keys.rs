//! The keys that raise the notch: volume, Caps Lock and the microphone's mute.
//! Thin wiring — the tap's decode is `aloud_hotkey::aux`, the CoreAudio work is
//! `aloud_media::sound` — mapping what they report to whispers through
//! [`crate::islands::takeover`].

use aloud_hotkey::aux::{self, AuxEvent, VolumeKey};
use aloud_islands::{Island, IslandKind, IslandSource, IslandTier};
use aloud_media::sound::{self, Output, VolumeAction};
use aloud_store::settings::NotchEvents;
use tauri::{AppHandle, Manager};

/// Tells the key tap whether to eat the volume keys. Called at launch and on
/// every settings save, so the switch needs no restart. Replacement needs the
/// key whispers on: an eaten key with nothing drawn is a dead key.
pub fn apply_settings(events: &NotchEvents) {
    aux::set_replace_volume(events.replace_volume_hud && events.huds);
}

pub fn start(app: &AppHandle) {
    if let Some(state) = app.try_state::<crate::AppState>() {
        if let Ok(settings) = state.settings.lock() {
            apply_settings(&settings.notch_events);
        }
    }

    // Taken before the tap starts (the listener thread is spawned after this),
    // or the first keys would have nowhere to go.
    if let Some(events) = aux::events() {
        let app = app.clone();
        // Its own thread: CoreAudio blocks, and the tap's must not.
        let spawned = std::thread::Builder::new()
            .name("aloud-keys".into())
            .spawn(move || {
                for event in events {
                    match event {
                        AuxEvent::Volume {
                            key,
                            fine,
                            swallowed,
                        } => volume(&app, key, fine, swallowed),
                        AuxEvent::CapsLock(on) => crate::islands::takeover(&app, caps(on)),
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(%e, "could not start the key watcher");
        }
    }

    let handle = app.clone();
    if let Err(e) = sound::watch_mic(move |muted| crate::islands::takeover(&handle, mic(muted))) {
        tracing::warn!(%e, "could not watch the microphone");
    }
}

fn volume(app: &AppHandle, key: VolumeKey, fine: bool, swallowed: bool) {
    if !swallowed {
        // macOS handled it, and drew its own HUD. If replacement is on, this
        // is the way back from an output with no volume: the key is eaten
        // again as soon as the default output has one.
        if replacement_on(app) {
            aux::set_volume_controllable(sound::has_output_volume());
        }
        return;
    }
    let action = match key {
        VolumeKey::Up => VolumeAction::Up,
        VolumeKey::Down => VolumeAction::Down,
        VolumeKey::Mute => VolumeAction::ToggleMute,
    };
    match sound::nudge_output(action, fine) {
        Ok(output) => crate::islands::takeover(app, volume_island(output)),
        Err(e) => {
            // ponytail: the key that found this out is already eaten; later ones
            // pass through to macOS. Re-posting the event is the upgrade.
            tracing::warn!(%e, "could not move the volume");
            aux::set_volume_controllable(sound::has_output_volume());
        }
    }
}

fn replacement_on(app: &AppHandle) -> bool {
    app.try_state::<crate::AppState>()
        .and_then(|s| {
            s.settings
                .lock()
                .ok()
                .map(|s| s.notch_events.replace_volume_hud)
        })
        .unwrap_or(false)
}

fn hud(id: &str, icon: &str, title: &str, short: Option<&str>) -> Island {
    Island {
        id: id.into(),
        kind: IslandKind::Alert,
        icon: Some(icon.into()),
        title: title.into(),
        short: short.map(Into::into),
        tier: Some(IslandTier::Whisper),
        source: IslandSource::Hud,
        ..Island::default()
    }
}

fn volume_island(output: Output) -> Island {
    let muted = output.muted;
    Island {
        level: Some(output.percent()),
        ..hud(
            "hud:volume",
            if muted { "mute" } else { "volume" },
            "Volume",
            muted.then_some("Muted"),
        )
    }
}

fn caps(on: bool) -> Island {
    hud(
        "hud:caps",
        "caps",
        "Caps Lock",
        Some(if on { "Caps Lock" } else { "Caps off" }),
    )
}

fn mic(muted: bool) -> Island {
    hud(
        "hud:mic",
        // The glyph carries the state, so the webview can colour it (red muted).
        if muted { "mic-off" } else { "mic" },
        "Microphone",
        Some(if muted { "Mic muted" } else { "Mic on" }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The failure this guards: a muted output drawing its remembered level,
    /// which reads as "the volume is up and nothing plays".
    #[test]
    fn a_muted_volume_whisper_is_empty_and_says_so() {
        let island = volume_island(Output {
            level: 0.5,
            muted: true,
        });
        assert_eq!(island.level, Some(0));
        assert_eq!(island.icon.as_deref(), Some("mute"));
        assert_eq!(island.short.as_deref(), Some("Muted"));
    }

    #[test]
    fn key_whispers_are_hud_whispers_with_a_stable_id_each() {
        for island in [
            volume_island(Output {
                level: 0.3,
                muted: false,
            }),
            caps(true),
            mic(true),
        ] {
            assert_eq!(island.source, IslandSource::Hud);
            assert_eq!(island.tier, Some(IslandTier::Whisper));
            assert!(island.id.starts_with("hud:"));
        }
        assert_eq!(caps(false).short.as_deref(), Some("Caps off"));
        assert_eq!(mic(false).short.as_deref(), Some("Mic on"));
    }
}
