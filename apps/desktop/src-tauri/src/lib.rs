mod agent;
mod commands;
mod islands;
mod listen;
mod media; // notch-media
mod meeting;
mod notch;
mod overlay;
mod pipeline;
mod quota;
mod sessions;
mod sound;
mod tray;

use aloud_store::Settings;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri_specta::{collect_commands, collect_events, Builder, Event};

/// How often to re-check for Accessibility permission while waiting for the
/// user to grant it. macOS offers no notification, so polling is the only way.
const PERMISSION_POLL: std::time::Duration = std::time::Duration::from_secs(1);

/// Emitted once hotkeys are live, so the UI can clear its permission banner
/// without the user restarting the app.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "hotkeys-permitted")]
pub struct HotkeysPermitted(pub bool);

/// Pushed when a background revalidation finds the connector list has changed —
/// the pane paints from cache, so this is how a browser connection reaches a
/// window that already rendered.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "connectors-updated")]
pub struct ConnectorsUpdated(pub Vec<commands::Connector>);

/// Shared app state, injected into commands via `tauri::State`.
pub struct AppState {
    pub settings: Arc<Mutex<Settings>>,
    pub pipeline: Arc<pipeline::Pipeline>,
    pub meeting: Arc<Mutex<Option<meeting::Meeting>>>,
    /// Some while always-listening mode is on.
    pub listener: Mutex<Option<listen::Listener>>,
    /// Shared with the listener thread, so a rebind from Settings applies
    /// without a restart.
    pub hotkey_config: Arc<Mutex<aloud_hotkey::model::Config>>,
    /// Arms binding capture in the listener thread. The recorder pane sets it;
    /// the listener clears it as soon as a gesture commits.
    pub hotkey_capture: aloud_hotkey::CaptureSlot,
    pub connectors: commands::ConnectorCache,
    /// Per-stage latency, written by the tracing layer and read by the dev panel.
    pub timings: Arc<Mutex<aloud_store::Timings>>,
    /// Measured once on the main thread at launch; empty without a notch.
    pub notch: Vec<notch::Notch>,
}

pub fn run() {
    // Spans go two places: the terminal, and `timings.sqlite` so the waterfall
    // survives the process.
    let timings = aloud_notes::data_dir()
        .and_then(|dir| aloud_store::Timings::open(&dir.join("timings.sqlite")))
        .unwrap_or_else(|e| {
            eprintln!("timings disabled: {e:#}");
            aloud_store::Timings::open_in_memory().expect("in-memory sqlite")
        });
    let timings = Arc::new(Mutex::new(timings));

    // The updater compiles rustls with `ring`, reqwest with `aws-lc-rs`; with
    // both in, rustls cannot pick one and panics the first time something asks
    // for the default — Deepgram's socket, which died silently in its task and
    // left the bar saying "Listening" while hearing nothing.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "aloud=info,aloud_hotkey=info".into()),
            ),
        )
        // Unfiltered on purpose: an env filter tuned for readable logs would
        // silently punch holes in the waterfall.
        .with(aloud_store::TimingLayer::new(timings.clone()))
        .init();

    let builder = specta_builder();

    // Regenerate the TypeScript bindings on every debug run so the frontend
    // never drifts from the Rust command surface.
    #[cfg(debug_assertions)]
    export_bindings(&builder).expect("failed to export typescript bindings");

    build_app(builder, timings);
}

/// The command and event surface, in one place so `run` and the bindings test
/// at the bottom of this file cannot end up describing different apps.
fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::get_settings,
            commands::update_settings,
            commands::get_stats,
            commands::search_notes,
            commands::recent_notes,
            commands::default_prompt,
            commands::test_prompt,
            commands::input_devices,
            commands::installed_apps,
            commands::finder_search,
            commands::finder_reveal,
            commands::arm_file_drag,
            commands::disarm_file_drag,
            commands::hotkeys_permitted,
            commands::permissions,
            commands::open_permission_settings,
            commands::list_permissions,
            commands::set_permission,
            commands::build_info,
            commands::overlay_displays,
            commands::recent_timings,
            commands::history_recent,
            commands::history_delete,
            commands::history_clear,
            commands::history_update_final,
            commands::history_retry_transcription,
            commands::capture_hotkey,
            commands::cancel_hotkey_capture,
            commands::meeting_status,
            commands::toggle_meeting,
            commands::debug_overlay,
            commands::preview_sound,
            commands::launch_at_login,
            commands::set_launch_at_login,
            commands::list_connectors,
            commands::search_connectors,
            commands::connect_connector,
            commands::disconnect_connector,
            overlay::set_panel_box,
            overlay::set_panel_interactive,
            overlay::close_browse,
            islands::islands,
            islands::dismiss_island,
            sessions::agent_hook_status,
            sessions::install_agent_hooks,
            sessions::uninstall_agent_hooks,
            sessions::resolve_permission,
            sessions::spawn_coding_agent,
            sessions::list_local_repos,
            sessions::list_repo_branches,
            sessions::read_agent_transcript,
            quota::agent_quota,
            // notch-media: the Media section's commands and event.
            media::media_now_playing,
            media::media_watch,
            media::media_control,
            media::media_seek,
            media::media_artwork,
            media::media_lyrics,
            media::sound_state,
            media::sound_set_default,
            media::sound_set_mic_muted,
            media::bluetooth_devices,
            media::bluetooth_connect,
            media::open_automation_settings,
            agent::confirm_agent,
            agent::arm_agent,
            agent::hold_agent,
            agent::answer_agent,
            agent::close_agent,
            agent::show_result,
            agent::undo_tidy,
        ])
        .events(collect_events![
            pipeline::Phase,
            meeting::MeetingPhase,
            agent::AgentEvent,
            overlay::AudioLevel,
            overlay::OverlayError,
            overlay::DictationPartial,
            overlay::ModelDownload,
            overlay::OverlayStyleChanged,
            overlay::OverlayDismiss,
            overlay::NotchHover,
            islands::IslandChanged,
            sessions::AgentSessions,
            sessions::AgentPermission,
            sessions::AgentPermissionResolved,
            media::MediaNowPlaying, // notch-media
            HotkeysPermitted,
            ConnectorsUpdated,
        ])
}

/// Writes `src/lib/bindings.ts`. The path is relative to this crate's
/// directory, which is why `cargo run -p aloud` has to be invoked from
/// `src-tauri` — `pnpm dev` does that for you.
fn export_bindings(builder: &Builder<tauri::Wry>) -> Result<(), specta_typescript::Error> {
    builder.export(
        specta_typescript::Typescript::default(),
        "../src/lib/bindings.ts",
    )
}

fn build_app(builder: Builder<tauri::Wry>, timings: Arc<Mutex<aloud_store::Timings>>) {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            spawn_update_check(app.handle().clone());

            // Before loading settings: a first launch should find the Swift
            // app's notes and keys already in place.
            match aloud_store::migrate::run() {
                Ok(report) if report.source.is_some() => {
                    tracing::info!(
                        notes = report.notes_copied,
                        settings = report.settings_imported,
                        "imported data from the Swift app"
                    );
                }
                // Never block startup on a failed import.
                Err(e) => tracing::warn!(%e, "could not import legacy data"),
                _ => {}
            }

            let settings = Arc::new(Mutex::new(Settings::load()));
            let pipeline = Arc::new(pipeline::Pipeline::new(
                app.handle().clone(),
                settings.clone(),
            )?);
            // Load the transcription model now rather than inside the user's
            // first dictation. Returns immediately; the work is spawned.
            pipeline.warm_up();
            meeting::recover_unsaved();
            let meeting: Arc<Mutex<Option<meeting::Meeting>>> = Arc::new(Mutex::new(None));
            let hotkey_config = Arc::new(Mutex::new(
                settings
                    .lock()
                    .map(|s| s.hotkeys.clone())
                    .unwrap_or_default(),
            ));
            let hotkey_capture: aloud_hotkey::CaptureSlot = Default::default();
            app.manage(AppState {
                settings: settings.clone(),
                pipeline: pipeline.clone(),
                meeting: meeting.clone(),
                listener: Mutex::new(None),
                hotkey_config: hotkey_config.clone(),
                hotkey_capture: hotkey_capture.clone(),
                connectors: Default::default(),
                timings: timings.clone(),
                // Tauri's setup callback is the main thread, which NSScreen requires.
                notch: notch::notched_screens(),
            });
            app.manage(agent::PendingConfirmation::default());
            app.manage(agent::PendingQuestion::default());

            // Main thread required. A floating-level window is clamped below
            // the menu bar, which the wings style has to sit in.
            overlay::raise_over_menu_bar(app.handle());
            // Drag-out for search results. Main thread, needs the overlay view.
            overlay::install_drag(app.handle());

            // The notch is permanent: an agent can start at any time, and a
            // notch that only exists during a dictation cannot show one.
            overlay::show_notch(app.handle());

            // Outside-click closes the panel, popover-style: the overlay only
            // holds key focus while a panel is open, so losing it means the
            // user clicked elsewhere. The click reaches the app behind because
            // the canvas no longer catches clicks outside the panel rect.
            if let Some(ov) = app.get_webview_window("overlay") {
                let handle = app.handle().clone();
                ov.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        overlay::dismiss_on_focus_loss(&handle);
                    }
                });
            }

            sessions::start(app.handle());
            media::start(app.handle()); // notch-media

            tray::init(app)?;

            // Off the critical path: the App Rules picker otherwise pays for
            // ~70 `sips` spawns on first click.
            std::thread::spawn(|| {
                let _ = aloud_selection::installed_apps();
            });

            commands::warm_connectors(app.handle().clone());

            spawn_hotkey_listener(
                app.handle().clone(),
                settings,
                pipeline,
                meeting,
                hotkey_config,
                hotkey_capture,
            );

            // Closing must hide, not destroy: a destroyed window makes
            // `get_webview_window("main")` return None, so the tray's "Open
            // Aloud" silently does nothing.
            if let Some(main) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                {
                    use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
                    let _ = apply_vibrancy(&main, NSVisualEffectMaterial::Sidebar, None, None);
                }

                let win = main.clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            // Menu-bar app: no dock icon, no taskbar entry.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // If the app died mid-dictation with the output muted, this is the
            // only thing that puts the level back.
            if let Err(e) = aloud_volume::restore() {
                tracing::warn!(%e, "could not restore the system volume from a previous run");
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            // Double-clicking the app while it is already running raises the
            // window — the only way back to settings once the menu-bar icon is
            // hidden, since there is no dock icon either.
            //
            // TODO: unverified — test this against a real build (`pnpm build`),
            // not `pnpm dev`. `applicationShouldHandleReopen` is delivered by
            // LaunchServices to a bundled `.app`, so a bare `cargo run` binary
            // can never raise it and no test here can cover it. Steps: turn
            // "Show menu bar icon" off, close the window, then double-click
            // Aloud in Applications — the window must come back. If it does
            // not, hiding the icon is a one-way door out of the app and the
            // toggle has to go behind a settings hotkey (see 1.4 / 2.1).
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                tray::show_main(_app);
            }
        });
}

/// Checks for an update on launch. Best-effort — a failure is logged, never
/// surfaced, so a release server being down never blocks the app.
fn spawn_update_check(app: tauri::AppHandle) {
    // No pubkey means the updater cannot verify a download; skip the whole thing.
    let configured = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .is_some_and(|k| !k.trim().is_empty());
    if !configured {
        tracing::debug!("updater: no pubkey configured; skipping update check");
        return;
    }

    tauri::async_runtime::spawn(async move {
        use tauri_plugin_updater::UpdaterExt;
        let updater = match app.updater() {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(%e, "updater unavailable");
                return;
            }
        };
        match updater.check().await {
            Ok(Some(update)) => {
                tracing::info!(version = %update.version, "update available; downloading");
                // ponytail: silent download-and-install on next launch. A
                // "restart now?" prompt is the upgrade path once there is a UI
                // surface for it; for a menu-bar app, quiet is the right default.
                if let Err(e) = update.download_and_install(|_, _| {}, || {}).await {
                    tracing::warn!(%e, "update install failed");
                }
            }
            Ok(None) => tracing::debug!("updater: already up to date"),
            Err(e) => tracing::warn!(%e, "update check failed"),
        }
    });
}

/// Runs the key hook on its own thread: both backends block forever (a run
/// loop on macOS, a message pump on Windows).
fn spawn_hotkey_listener(
    app: tauri::AppHandle,
    settings: Arc<Mutex<Settings>>,
    pipeline: Arc<pipeline::Pipeline>,
    meeting: Arc<Mutex<Option<meeting::Meeting>>>,
    config: Arc<Mutex<aloud_hotkey::model::Config>>,
    capture: aloud_hotkey::CaptureSlot,
) {
    let settings = settings.clone();
    std::thread::spawn(move || {
        // macOS gives no signal when Accessibility is granted and does not
        // restart the app, so wait for it rather than giving up at launch.
        if !aloud_hotkey::is_permitted() {
            tracing::warn!("accessibility permission not granted; waiting for it");
            while !aloud_hotkey::is_permitted() {
                std::thread::sleep(PERMISSION_POLL);
            }
            tracing::info!("accessibility permission granted; starting hotkeys");
        }
        if let Err(e) = HotkeysPermitted(true).emit(&app) {
            tracing::warn!(%e, "could not publish hotkey permission");
        }

        let busy = pipeline.busy_flag();
        let result = aloud_hotkey::listen(config, busy, capture, move |event, _held| {
            handle_hotkey(&app, &pipeline, &settings, &meeting, event);
        });
        if let Err(e) = result {
            tracing::error!(%e, "hotkey listener stopped");
        }
    });
}

fn handle_hotkey(
    app: &tauri::AppHandle,
    pipeline: &Arc<pipeline::Pipeline>,
    settings: &Arc<Mutex<Settings>>,
    meeting: &Arc<Mutex<Option<meeting::Meeting>>>,
    event: aloud_hotkey::model::Event,
) {
    use aloud_hotkey::model::{Event, Role};

    match event {
        Event::Activated(Role::Hold) => {
            // Note anything selected before recording begins: with a selection
            // the utterance rewrites it, without one it is dictated. A poisoned
            // lock falls back to looking rather than dropping the selection.
            if settings.lock().map_or(true, |s| s.command_mode_enabled) {
                pipeline.note_selection_if_any();
            }
            if let Err(e) = pipeline.start(Role::Hold) {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Deactivated(Role::Hold) => {
            // Dropped if a more-specific overlapping binding (e.g. `⌥fn`) took
            // the session over: stopping here would tear out from under it.
            if !pipeline.owns(Role::Hold) {
                return;
            }
            // Transcription is async and the hook thread must not block: a
            // stalled hook stops delivering key events system-wide.
            let pipeline = pipeline.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pipeline.stop_and_dictate().await {
                    tracing::error!(%e, "dictation failed");
                }
            });
        }
        // Tap to start, tap again to stop: no Deactivated edge to hang it on.
        Event::Activated(Role::Toggle) => {
            if pipeline.is_recording() {
                let pipeline = pipeline.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = pipeline.stop_and_paste().await {
                        tracing::error!(%e, "dictation failed");
                    }
                });
            } else if let Err(e) = pipeline.start(Role::Toggle) {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Activated(Role::Ask) => {
            if let Err(e) = pipeline.start(Role::Ask) {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Deactivated(Role::Ask) => {
            if !pipeline.owns(Role::Ask) {
                return;
            }
            let pipeline = pipeline.clone();
            // ponytail: Ask is a second Command key since the merge. The role is
            // kept so existing bindings fire; fold it into Agent with a settings
            // migration the next time the hotkey schema changes.
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pipeline.stop_and_agent().await {
                    tracing::error!(%e, "command failed");
                }
            });
        }
        Event::Activated(Role::Rewrite) => {
            // Before recording moves focus. The thorough read — an explicit
            // rewrite is worth the clipboard fallback's cost.
            pipeline.capture_selection();
            if let Err(e) = pipeline.start(Role::Rewrite) {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Deactivated(Role::Rewrite) => {
            if !pipeline.owns(Role::Rewrite) {
                return;
            }
            let pipeline = pipeline.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pipeline.stop_and_dictate().await {
                    tracing::error!(%e, "rewrite failed");
                }
            });
        }
        Event::Activated(Role::Agent) => {
            if let Err(e) = pipeline.start(Role::Agent) {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Deactivated(Role::Agent) => {
            if !pipeline.owns(Role::Agent) {
                return;
            }
            let pipeline = pipeline.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pipeline.stop_and_agent().await {
                    tracing::error!(%e, "agent mode failed");
                }
            });
        }
        Event::Activated(Role::CopyAgain) => {
            if let Err(e) = pipeline.paste_again() {
                tracing::error!(%e, "could not paste again");
            }
        }
        Event::Activated(Role::MeetingNotes) => toggle_meeting(app, settings, meeting),
        Event::Cancelled => pipeline.cancel(),
        other => tracing::debug!(?other, "unhandled hotkey event"),
    }
}

/// Starts a meeting-notes session, or stops and saves the running one.
pub(crate) fn toggle_meeting(
    app: &tauri::AppHandle,
    settings: &Arc<Mutex<Settings>>,
    meeting: &Arc<Mutex<Option<meeting::Meeting>>>,
) {
    // One guard across the whole toggle: the tray fires this on the main thread
    // and the hotkey on the event-tap thread, so releasing between the take and
    // the store lets both open a recorder and orphans the first.
    let mut slot = meeting.lock().unwrap();
    match slot.take() {
        Some(session) => {
            // Off-thread: summarizing is a network round-trip.
            tauri::async_runtime::spawn(async move {
                match session.stop().await {
                    Ok(path) => tracing::info!(note = %path.display(), "meeting note saved"),
                    Err(e) => tracing::error!(%e, "saving meeting note failed"),
                }
            });
        }
        None => {
            let settings_snapshot = settings.lock().map(|s| s.clone()).unwrap_or_default();
            let pipeline = app.state::<AppState>().pipeline.clone();
            match meeting::Meeting::start(app.clone(), settings_snapshot, pipeline) {
                Ok(session) => *slot = Some(session),
                Err(e) => tracing::error!(%e, "starting meeting failed"),
            }
        }
    }
}

/// Turns always-listening mode on, or off. Off drops the listener, which
/// closes the mic and stops its queue.
pub(crate) fn toggle_listening(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let Ok(mut slot) = state.listener.lock() else {
        return;
    };
    if slot.take().is_some() {
        return;
    }
    let settings = state.settings.lock().map(|s| s.clone()).unwrap_or_default();
    match listen::Listener::start(app.clone(), settings, state.pipeline.clone()) {
        Ok(listener) => *slot = Some(listener),
        Err(e) => tracing::error!(%e, "starting always-listening failed"),
    }
}

/// Turns always-listening off, if it is on — "stop listening" said aloud.
pub(crate) fn stop_listening(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let stopped = state.listener.lock().ok().and_then(|mut slot| slot.take());
    // Dropped outside the lock: closing the mic waits on the audio thread.
    drop(stopped);
    tray::refresh(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regenerates `src/lib/bindings.ts` from the command surface.
    ///
    /// `run` does this too, but only when the app actually launches — which
    /// means changing a command's signature could not be typechecked without
    /// starting the GUI, and a half-updated `bindings.ts` would be committed
    /// against a Rust surface nobody had exported. `cargo test -p aloud` is
    /// enough now.
    ///
    /// It writes rather than asserts: the file is generated, so a diff after
    /// running the tests is the signal that the frontend has to catch up.
    #[test]
    fn bindings_match_the_command_surface() {
        export_bindings(&specta_builder()).expect("exporting typescript bindings");
    }
}
