//! The menu-bar menu — the app's only always-visible surface, since there is no
//! dock icon.
//!
//! Tauri offers no way to mutate one menu item in place, so any change means
//! rebuilding the whole menu: [`refresh`], called on every phase change.

use crate::AppState;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Manager,
};

const TRAY_ID: &str = "aloud";
const HISTORY_ITEMS: usize = 10;
/// Menu rows are one line: long transcripts are cut to this before the ellipsis.
const SNIPPET_CHARS: usize = 48;

pub fn init(app: &App) -> tauri::Result<()> {
    let settings = app.state::<AppState>();
    let visible = settings
        .settings
        .lock()
        .map(|s| s.show_menu_bar_icon)
        .unwrap_or(true);
    if visible {
        create(app.handle())?;
    }
    Ok(())
}

fn create(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&build(app)?)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event)
        .build(app)?;
    Ok(())
}

/// Shows or hides the menu-bar icon without a restart.
///
/// Hiding is a one-way door but for the `RunEvent::Reopen` arm in `lib.rs`,
/// which is **not yet verified against a real build** — see the TODO there.
pub fn set_visible(app: &AppHandle, visible: bool) {
    let handle = app.clone();
    // Creating and destroying a status item is main-thread-only on macOS; this
    // arrives on whichever thread saved the settings.
    let _ = app.run_on_main_thread(move || {
        let exists = handle.tray_by_id(TRAY_ID).is_some();
        match (visible, exists) {
            (true, false) => {
                if let Err(e) = create(&handle) {
                    tracing::warn!(%e, "could not show the menu bar icon");
                }
            }
            (false, true) => {
                handle.remove_tray_by_id(TRAY_ID);
            }
            _ => {}
        }
    });
}

/// Rebuilds the menu against current state. Cheap enough to call on every
/// phase change; a no-op before the tray exists.
pub fn refresh(app: &AppHandle) {
    let handle = app.clone();
    // Menu mutation is main-thread-only on macOS; phases arrive on tokio
    // workers and the hotkey thread.
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id(TRAY_ID) else {
            return;
        };
        match build(&handle) {
            Ok(menu) => {
                if let Err(e) = tray.set_menu(Some(menu)) {
                    tracing::warn!(%e, "could not set tray menu");
                }
            }
            Err(e) => tracing::warn!(%e, "could not rebuild tray menu"),
        }
    });
}

fn build(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    let recording = state.pipeline.is_recording();
    let in_meeting = state.meeting.lock().map(|m| m.is_some()).unwrap_or(false);
    let listening = state.listener.lock().map(|l| l.is_some()).unwrap_or(false);
    let last = state.pipeline.last_transcript();

    let menu = Menu::new(app)?;

    menu.append(&disabled(app, "status", status_line(app))?)?;
    let stats = state.pipeline.stats();
    if stats.total_sessions > 0 {
        menu.append(&disabled(app, "stats", stats_line(&stats))?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // A journal capture is a recording too, but its stop keeps the words
    // instead of pasting them: it gets its own row, and the dictate row waits.
    let capturing = state.pipeline.owns(aloud_hotkey::model::Role::Journal);
    menu.append(&MenuItem::with_id(
        app,
        "dictate",
        if recording && !capturing {
            "Stop Recording"
        } else {
            "Start Dictating"
        },
        !capturing,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "journal",
        if capturing {
            "Stop & Save Thought"
        } else {
            "Capture a Thought"
        },
        capturing || !recording,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "meeting",
        if in_meeting {
            "Stop Meeting Notes & Save"
        } else {
            "Start Meeting Notes"
        },
        true,
        None::<&str>,
    )?)?;

    // Always-listening hears commands, so it is the voice agent's. A listener
    // still running keeps its row, so it can always be stopped from here.
    if settings.labs.voice_agent || listening {
        menu.append(&MenuItem::with_id(
            app,
            "listen",
            if listening {
                "Stop Always Listening"
            } else {
                "Always Listening"
            },
            true,
            None::<&str>,
        )?)?;
    }

    // Greyed out before the first dictation rather than silently doing nothing.
    menu.append(&MenuItem::with_id(
        app,
        "paste_again",
        "Paste Again",
        last.is_some(),
        None::<&str>,
    )?)?;
    if let Some(text) = &last {
        menu.append(&disabled(app, "last", format!("“{}”", snippet(text)))?)?;
    }

    menu.append(&MenuItem::with_id(
        app,
        "vocab_from_clipboard",
        "Add Clipboard Word to Vocabulary",
        true,
        None::<&str>,
    )?)?;

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&history_submenu(app, &state)?)?;
    menu.append(&microphone_submenu(app, &settings)?)?;

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "show",
        "Open Aloud",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "settings",
        "Settings…",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "setup",
        "Re-run Setup…",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Quit Aloud",
        true,
        Some("CmdOrCtrl+Q"),
    )?)?;

    Ok(menu)
}

/// The last ten transcripts; clicking one copies it.
fn history_submenu(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
) -> tauri::Result<Submenu<tauri::Wry>> {
    let submenu = Submenu::with_id(app, "history", "History", true)?;
    let entries = state.pipeline.history(HISTORY_ITEMS).unwrap_or_default();

    if entries.is_empty() {
        submenu.append(&disabled(app, "history_empty", "Nothing yet")?)?;
        return Ok(submenu);
    }
    for entry in entries {
        submenu.append(&MenuItem::with_id(
            app,
            format!("history:{}", entry.id),
            snippet(&entry.final_transcript),
            true,
            None::<&str>,
        )?)?;
    }
    Ok(submenu)
}

/// Input devices, with the selected one ticked. "System Default" is the
/// `None` case, matching `Settings::selected_microphone`.
fn microphone_submenu(
    app: &AppHandle,
    settings: &aloud_store::Settings,
) -> tauri::Result<Submenu<tauri::Wry>> {
    let submenu = Submenu::with_id(app, "mics", "Microphone", true)?;
    let selected = settings.selected_microphone.as_deref();

    submenu.append(&CheckMenuItem::with_id(
        app,
        "mic:",
        "System Default",
        true,
        selected.is_none(),
        None::<&str>,
    )?)?;
    for device in aloud_audio::input_devices().unwrap_or_default() {
        submenu.append(&CheckMenuItem::with_id(
            app,
            format!("mic:{}", device.name),
            &device.name,
            true,
            selected == Some(device.name.as_str()),
            None::<&str>,
        )?)?;
    }
    Ok(submenu)
}

fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id.as_ref();

    // Prefixed ids carry their payload: the history row's id, the mic's name.
    if let Some(raw) = id.strip_prefix("history:") {
        copy_history_entry(app, raw);
        return;
    }
    if let Some(name) = id.strip_prefix("mic:") {
        select_microphone(app, name);
        return;
    }

    match id {
        "dictate" => toggle_dictation(app),
        "journal" => {
            if let Err(e) = crate::journal::toggle(&app.state::<AppState>().pipeline) {
                tracing::warn!(%e, "journal capture did not start");
            }
            refresh(app);
        }
        "meeting" => {
            let state = app.state::<AppState>();
            crate::toggle_meeting(app, &state.settings, &state.meeting);
            refresh(app);
        }
        "listen" => {
            crate::toggle_listening(app);
            refresh(app);
        }
        "paste_again" => {
            if let Err(e) = app.state::<AppState>().pipeline.paste_again() {
                tracing::warn!(%e, "paste again failed");
            }
        }
        "vocab_from_clipboard" => add_clipboard_to_vocabulary(app),
        "show" => show_main(app),
        "settings" => {
            show_main(app);
            if let Err(e) = app.emit("open-settings", ()) {
                tracing::warn!(%e, "could not ask the window to open settings");
            }
        }
        // The window shows the wizard whenever `onboarded` is false.
        "setup" => {
            let state = app.state::<AppState>();
            let mut settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
            settings.onboarded = false;
            if let Err(e) = settings.save() {
                tracing::warn!(%e, "could not reset onboarding");
                return;
            }
            if let Ok(mut current) = state.settings.lock() {
                *current = settings;
            }
            show_main(app);
            if let Err(e) = app.emit("rerun-setup", ()) {
                tracing::warn!(%e, "could not ask the window to show setup");
            }
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

/// The same flow the hold hotkey drives, so dictation still works before
/// Accessibility is granted.
fn toggle_dictation(app: &AppHandle) {
    let pipeline = app.state::<AppState>().pipeline.clone();
    if pipeline.is_recording() {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = pipeline.stop_and_paste().await {
                tracing::error!(%e, "dictation failed");
            }
        });
    } else if let Err(e) = pipeline.start(aloud_hotkey::model::Role::Toggle) {
        tracing::error!(%e, "could not start recording");
    }
}

fn copy_history_entry(app: &AppHandle, raw_id: &str) {
    let Ok(id) = raw_id.parse::<i64>() else {
        return;
    };
    let state = app.state::<AppState>();
    let entries = state.pipeline.history(HISTORY_ITEMS).unwrap_or_default();
    let Some(entry) = entries.into_iter().find(|e| e.id == id) else {
        return;
    };
    if let Err(e) = aloud_paste::set_clipboard(&entry.final_transcript) {
        tracing::warn!(%e, "could not copy transcript");
    }
}

/// Teaches the transcriber a spelling without opening Settings: copy the word
/// wherever it is spelled correctly, then pick this.
///
/// ponytail: no checkmark flash, unlike the Swift menu. Every phase change
/// rebuilds the menu wholesale (see [`refresh`]), so transient state does not
/// survive long enough to be seen; revisit if Tauri ever allows mutating one
/// item in place.
fn add_clipboard_to_vocabulary(app: &AppHandle) {
    let Ok(text) = aloud_paste::clipboard_text() else {
        tracing::warn!("clipboard holds no text");
        return;
    };

    let state = app.state::<AppState>();
    let mut settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    // Split, not stored whole: a pasted paragraph kept as one "term" would be
    // prefixed to every subsequent cleanup request.
    let added = settings.add_vocabulary_terms(&text);
    if added == 0 {
        tracing::info!("clipboard holds nothing new to add to the vocabulary");
        return;
    }
    if let Err(e) = settings.save() {
        tracing::warn!(%e, "could not save vocabulary");
        return;
    }
    let Ok(mut current) = state.settings.lock() else {
        return;
    };
    *current = settings;
}

fn select_microphone(app: &AppHandle, name: &str) {
    let state = app.state::<AppState>();
    let mut settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    // The "System Default" row carries an empty name.
    settings.selected_microphone = (!name.is_empty()).then(|| name.to_string());

    if let Err(e) = settings.save() {
        tracing::warn!(%e, "could not save microphone choice");
        return;
    }
    if let Ok(mut current) = state.settings.lock() {
        *current = settings;
    }
    refresh(app);
}

fn status_line(app: &AppHandle) -> String {
    use crate::pipeline::Phase;
    match app.state::<AppState>().pipeline.phase() {
        Phase::Recording => "Listening…",
        Phase::Transcribing => "Transcribing…",
        Phase::Error => "Last dictation failed",
        Phase::NoSpeech => "No speech detected",
        Phase::Saved => "Saved to your journal",
        Phase::Idle | Phase::Done | Phase::Listening => "Ready",
    }
    .to_string()
}

/// "1204 words · ~12 min saved · 3-day streak", skipping the streak at zero.
fn stats_line(stats: &aloud_store::Stats) -> String {
    let minutes = stats.time_saved_seconds() / 60.0;
    let saved = if minutes < 60.0 {
        format!("~{} min saved", minutes.round() as u64)
    } else {
        format!("~{:.1} h saved", minutes / 60.0)
    };

    let mut line = format!("{} words · {saved}", stats.total_words);
    if stats.current_streak_days > 0 {
        line.push_str(&format!(" · {}-day streak", stats.current_streak_days));
    }
    line
}

/// A one-line preview: whitespace collapsed, cut on a char boundary so a
/// multi-byte character cannot be sliced in half.
fn snippet(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= SNIPPET_CHARS {
        return flat;
    }
    format!("{}…", flat.chars().take(SNIPPET_CHARS).collect::<String>())
}

/// A row that displays state rather than offering an action.
fn disabled(
    app: &AppHandle,
    id: &str,
    text: impl AsRef<str>,
) -> tauri::Result<MenuItem<tauri::Wry>> {
    MenuItem::with_id(app, id, text.as_ref(), false, None::<&str>)
}

/// Raises the main window — the single way back to it.
pub(crate) fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_collapses_whitespace_and_cuts_on_a_char_boundary() {
        assert_eq!(snippet("hello   there\nworld"), "hello there world");
        // Multi-byte characters straddling the cut must not panic or split.
        let long = "🦀".repeat(80);
        let cut = snippet(&long);
        assert!(cut.ends_with('…'));
        assert_eq!(cut.chars().count(), SNIPPET_CHARS + 1);
    }

    #[test]
    fn stats_line_drops_a_zero_streak_and_switches_to_hours() {
        let mut stats = aloud_store::Stats {
            total_words: 100,
            ..Default::default()
        };
        assert!(!stats_line(&stats).contains("streak"));
        assert!(stats_line(&stats).contains("min saved"));

        // 40 wpm means 10k words is over four hours of typing.
        stats.total_words = 10_000;
        stats.current_streak_days = 3;
        let line = stats_line(&stats);
        assert!(line.contains("h saved"), "got: {line}");
        assert!(line.contains("3-day streak"));
    }
}
