//! The notch and the voice agent: experiments beside the core app (dictation,
//! meetings, the journal), each behind a switch in Settings › Labs.
//!
//! Read through two atomics, so the hot paths — the hotkey thread, the takeover
//! gate, every watcher's tick — never take the settings lock. [`apply`] is the
//! only writer, from `setup` and from every settings save.

use aloud_hotkey::model::Config;
use aloud_store::Settings;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;
use tauri_specta::Event;

static NOTCH: AtomicBool = AtomicBool::new(false);
static VOICE_AGENT: AtomicBool = AtomicBool::new(false);
/// The notch's watchers start once, the first time it is on. Switching it off
/// parks them (each tick checks [`notch`]) rather than tearing them down, so
/// turning it back on is instant.
static NOTCH_STARTED: AtomicBool = AtomicBool::new(false);

/// Whether the notch is on. Everything that feeds or draws it checks this.
pub fn notch() -> bool {
    NOTCH.load(Ordering::Relaxed)
}

/// Whether the voice agent is on. Every path into a command run checks this.
pub fn voice_agent() -> bool {
    VOICE_AGENT.load(Ordering::Relaxed)
}

/// Sent to both windows when a switch flips, so the overlay can stop drawing
/// the notch and the main window can show or hide what belongs to it, without
/// waiting for the next settings read.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "labs-changed")]
pub struct LabsChanged {
    pub notch: bool,
    pub voice_agent: bool,
}

/// The hotkeys the listener should match: the user's, minus the Command keys
/// while the voice agent is off. Stripped rather than ignored on arrival, so a
/// Command key that shares a modifier with the dictate key cannot take the
/// session over and swallow the dictation.
pub fn hotkeys(settings: &Settings) -> Config {
    let mut config = settings.hotkeys.clone();
    if !settings.labs.voice_agent {
        config.agent = None;
        config.ask = None;
    }
    config
}

/// Brings the running app in line with `settings.labs`. Idempotent: called on
/// every save, and only a switch that actually flipped does anything.
pub fn apply(app: &AppHandle, settings: &Settings) {
    let labs = &settings.labs;
    let agent_was = VOICE_AGENT.swap(labs.voice_agent, Ordering::SeqCst);
    let notch_was = NOTCH.swap(labs.notch, Ordering::SeqCst);

    if agent_was && !labs.voice_agent {
        // Always-listening is the agent's: it must not keep hearing commands.
        crate::stop_listening(app);
        // A card waiting on an answer belongs to a run that can no longer go on.
        crate::agent::clear_pending(app);
    }

    // On the main thread whoever saved: the resting window is an NSWindow and
    // the Bluetooth watch delivers on the main run loop. Queued in order, so
    // a quick on-off-on lands as the last switch says.
    match (notch_was, labs.notch) {
        (false, true) => {
            let events = settings.notch_events.clone();
            on_main(app, move |app| {
                notch_on(app);
                // After the keys' consumer exists: swallowing a volume key
                // with nothing yet to act on it would just lose the key.
                crate::keys::apply_settings(&events);
            });
        }
        (true, false) => {
            // Stop eating the volume keys at once, not a tick later.
            crate::keys::apply_settings(&settings.notch_events);
            let settings = settings.clone();
            on_main(app, move |app| notch_off(app, &settings));
        }
        // Every other save: the HUD switches live in the notch's own settings.
        _ => crate::keys::apply_settings(&settings.notch_events),
    }
    // Every save: the notification mirror has a switch of its own, and needs
    // the notch on as well.
    if labs.notch {
        crate::nudges::apply(app, &settings.notch_events);
    }

    if agent_was != labs.voice_agent || notch_was != labs.notch {
        let changed = LabsChanged {
            notch: labs.notch,
            voice_agent: labs.voice_agent,
        };
        if let Err(e) = changed.emit(app) {
            tracing::warn!(%e, "could not announce the labs change");
        }
        crate::tray::refresh(app);
    }
}

fn on_main(app: &AppHandle, work: impl FnOnce(&AppHandle) + Send + 'static) {
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || work(&handle)) {
        tracing::warn!(%e, "could not switch the notch");
    }
}

fn notch_on(app: &AppHandle) {
    if !NOTCH_STARTED.swap(true, Ordering::SeqCst) {
        crate::sessions::start(app);
        crate::media::start(app);
        crate::clipboard::start(app);
        crate::mac::start(app);
        crate::today::init(app);
        // Takes the tap's aux event stream. Keys pressed before it is taken
        // have nowhere to go: they are dropped, never queued, so starting
        // after the tap (at launch, or when switched on later) is safe.
        crate::keys::start(app);
    }
    // Not once: the shelf's watchers stop when the notch goes off, so each
    // `on` restarts them. (The notification mirror is `apply`'s, every save.)
    crate::shelf::start_watchers(app);
    // Now Playing at once, not at the parked loop's next eight-second tick.
    crate::media::poll_now();
    crate::overlay::show_notch(app);
}

fn notch_off(app: &AppHandle, settings: &Settings) {
    // The notch's own Notification Center reader stops outright: it reads
    // other apps' words, and nothing is left to show them.
    let mut events = settings.notch_events.clone();
    events.app_notifications = false;
    crate::nudges::apply(app, &events);
    // A coding agent parked on an approval would wait a day for a card nobody
    // can see: hand each one back to the agent's own prompt.
    crate::sessions::release_all(app);
    crate::shelf::stop_watchers(app);
    crate::overlay::hide_notch(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use aloud_hotkey::model::{Binding, Key, Modifiers};
    use aloud_store::settings::Labs;

    fn with_command_keys(voice_agent: bool) -> Settings {
        let mut settings = Settings {
            labs: Labs {
                notch: false,
                voice_agent,
            },
            ..Default::default()
        };
        settings.hotkeys.agent = Some(Binding::modifier_chord(Key::Fn, Modifiers::new()));
        settings.hotkeys.ask = Some(Binding::modifier_chord(Key::LeftOption, Modifiers::new()));
        settings
    }

    /// The failure guarded: a Command key left bound with the agent off takes
    /// the session from the dictate key it overlaps and runs nothing.
    #[test]
    fn the_command_keys_are_unbound_while_the_agent_is_off() {
        let off = hotkeys(&with_command_keys(false));
        assert!(off.agent.is_none() && off.ask.is_none());
        assert!(off.hold.is_some(), "dictation keeps its key");

        let on = hotkeys(&with_command_keys(true));
        assert!(on.agent.is_some() && on.ask.is_some());
    }

    /// Stripping happens in the live config only: the saved bindings survive,
    /// so turning the agent back on restores the user's keys.
    #[test]
    fn turning_the_agent_off_keeps_the_saved_command_keys() {
        let settings = with_command_keys(false);
        let _ = hotkeys(&settings);
        assert!(settings.hotkeys.agent.is_some() && settings.hotkeys.ask.is_some());
    }
}
