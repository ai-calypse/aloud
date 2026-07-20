mod agent;
mod ask;
mod commands;
mod meeting;
mod overlay;
mod pipeline;
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

/// Pushed when a background revalidation finds the connector list has changed.
///
/// The Connectors pane paints from cache immediately, so this is how a service
/// connected in the browser reaches a window that already rendered.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "connectors-updated")]
pub struct ConnectorsUpdated(pub Vec<commands::Connector>);

/// Shared app state, injected into commands via `tauri::State`.
pub struct AppState {
    pub settings: Arc<Mutex<Settings>>,
    pub pipeline: Arc<pipeline::Pipeline>,
    /// The running meeting session, if any. A one-shot hotkey toggles it.
    pub meeting: Arc<Mutex<Option<meeting::Meeting>>>,
    /// The live hotkey config the listener reads. Shared with the listener
    /// thread so a rebind from Settings applies without a restart.
    pub hotkey_config: Arc<Mutex<aloud_hotkey::model::Config>>,
    /// Arms binding capture in the listener thread. The recorder pane sets it;
    /// the listener clears it as soon as a gesture commits.
    pub hotkey_capture: aloud_hotkey::CaptureSlot,
    /// Composio reads held across opens of the Connectors pane, so it paints
    /// from memory instead of a round-trip. See `aloud_agent::ResponseCache`.
    pub connectors: commands::ConnectorCache,
    /// Per-stage latency, written by the tracing layer and read by the dev
    /// panel. Shared with the layer, which owns the writes.
    pub timings: Arc<Mutex<aloud_store::Timings>>,
}

pub fn run() {
    // Spans go two places: the terminal, as before, and `timings.sqlite`, so
    // the waterfall survives the process and reads offline.
    let timings = aloud_notes::data_dir()
        .and_then(|dir| aloud_store::Timings::open(&dir.join("timings.sqlite")))
        .unwrap_or_else(|e| {
            eprintln!("timings disabled: {e:#}");
            aloud_store::Timings::open_in_memory().expect("in-memory sqlite")
        });
    let timings = Arc::new(Mutex::new(timings));

    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "aloud=info,aloud_hotkey=info".into()),
            ),
        )
        // Unfiltered on purpose: the layer records only spans that descend
        // from an utterance, and an env filter tuned for readable logs would
        // silently punch holes in the waterfall.
        .with(aloud_store::TimingLayer::new(timings.clone()))
        .init();

    let builder = Builder::<tauri::Wry>::new()
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
            commands::hotkeys_permitted,
            commands::recent_timings,
            commands::capture_hotkey,
            commands::cancel_hotkey_capture,
            commands::preview_sound,
            commands::launch_at_login,
            commands::set_launch_at_login,
            commands::list_connectors,
            commands::search_connectors,
            commands::connect_connector,
            commands::disconnect_connector,
            ask::close_ask,
            agent::confirm_agent,
            agent::close_agent,
        ])
        .events(collect_events![
            pipeline::Phase,
            meeting::MeetingPhase,
            ask::AskEvent,
            agent::AgentEvent,
            overlay::AudioLevel,
            overlay::OverlayError,
            overlay::ModelDownload,
            HotkeysPermitted,
            ConnectorsUpdated,
        ]);

    // Regenerate the TypeScript bindings on every debug run so the frontend
    // never drifts from the Rust command surface.
    #[cfg(debug_assertions)]
    builder
        .export(
            specta_typescript::Typescript::default(),
            "../src/lib/bindings.ts",
        )
        .expect("failed to export typescript bindings");

    tauri::Builder::default()
        // Opens the Composio OAuth URL when connecting a connector.
        .plugin(tauri_plugin_opener::init())
        // MacosLauncher is ignored off macOS; no launch args are needed.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            // Check for an update on launch, in the background. A no-op until a
            // signing pubkey is configured — see `check_for_update`.
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
                // Never block startup on a failed import: the user's data is
                // untouched either way, and they can still use the app.
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
            let meeting: Arc<Mutex<Option<meeting::Meeting>>> = Arc::new(Mutex::new(None));
            // The live hotkey config, shared between AppState (written on a
            // rebind) and the listener thread (which reads it every event).
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
                hotkey_config: hotkey_config.clone(),
                hotkey_capture: hotkey_capture.clone(),
                connectors: Default::default(),
                timings: timings.clone(),
            });
            app.manage(agent::PendingConfirmation::default());

            tray::init(app)?;

            // Warm the installed-apps cache off the critical path: the App
            // Rules picker then opens against a populated cache instead of
            // paying for ~70 `sips` spawns on first click.
            std::thread::spawn(|| {
                let _ = aloud_selection::installed_apps();
            });

            // Same for connectors, so the first open is warm too. Only for
            // users who have configured a key — otherwise there is nothing to
            // fetch and no reason to touch the network on launch.
            commands::warm_connectors(app.handle().clone());

            spawn_hotkey_listener(
                app.handle().clone(),
                settings,
                pipeline,
                meeting,
                hotkey_config,
                hotkey_capture,
            );

            // Closing the window must hide it, not destroy it. This is a
            // menu-bar app: the window is reopened from the tray, and a
            // destroyed window makes `get_webview_window("main")` return None,
            // so "Open Aloud" silently does nothing. Hide and prevent the close.
            if let Some(main) = app.get_webview_window("main") {
                // Native macOS Liquid Glass: a sidebar-material NSVisualEffectView
                // behind the transparent webview. The frontend paints the sidebar
                // and cards with translucent fills so the vibrancy shows through.
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

            // If the app died mid-dictation with the output muted, the saved
            // level is still on disk and this is the only thing that will put
            // it back — the user has no in-app control that would.
            if let Err(e) = aloud_volume::restore() {
                tracing::warn!(%e, "could not restore the system volume from a previous run");
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            // Reopening the app — double-clicking it in Finder while it is
            // already running — raises the window. This is the recovery path
            // that makes hiding the menu-bar icon safe: with no dock icon and
            // no status item, it is otherwise the only way back to settings.
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

/// Checks for an update on launch and installs one if the user agrees.
///
/// A no-op until a signing pubkey is configured in `tauri.conf.json`: without
/// it the updater cannot verify a download, so there is nothing safe to do.
/// The check itself is best-effort — a failure is logged, never surfaced, so a
/// release server being down never blocks the app.
fn spawn_update_check(app: tauri::AppHandle) {
    // The pubkey is baked into the config at build time; an empty one means
    // updates are not set up for this build, so skip the whole thing.
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
    // `config` is the same Arc AppState holds, so a rebind from Settings is
    // seen here on the next key event — no restart needed.
    let settings = settings.clone();
    std::thread::spawn(move || {
        // Granting Accessibility does not restart the app, and macOS gives no
        // signal when it happens, so wait for the grant rather than giving up
        // at launch. Without this the user grants permission, sees nothing
        // change, and has to quit and reopen to get working hotkeys.
        if !aloud_hotkey::is_permitted() {
            tracing::warn!("accessibility permission not granted; waiting for it");
            while !aloud_hotkey::is_permitted() {
                std::thread::sleep(PERMISSION_POLL);
            }
            tracing::info!("accessibility permission granted; starting hotkeys");
        }
        // Tell the UI so the banner can clear itself without a restart.
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
            // the utterance rewrites it, without one it is dictated. A
            // selection is the only signal — the extra-modifier gesture came
            // out, because the Rewrite hotkey covers rewriting on demand. A
            // poisoned lock falls back to looking, which is the behaviour that
            // predates the toggle, rather than dropping the selection silently.
            if settings.lock().map_or(true, |s| s.command_mode_enabled) {
                pipeline.note_selection_if_any();
            }
            if let Err(e) = pipeline.start() {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Deactivated(Role::Hold) => {
            // Transcription is async and the hook thread must not block: a
            // stalled hook stops delivering key events system-wide.
            let pipeline = pipeline.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pipeline.stop_and_dictate().await {
                    tracing::error!(%e, "dictation failed");
                }
            });
        }
        // Tap to start, tap again to stop — the same flow as Hold, but with two
        // presses instead of a press and a release, so there is no Deactivated
        // edge to hang the stop on.
        Event::Activated(Role::Toggle) => {
            if pipeline.is_recording() {
                let pipeline = pipeline.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = pipeline.stop_and_paste().await {
                        tracing::error!(%e, "dictation failed");
                    }
                });
            } else if let Err(e) = pipeline.start() {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Activated(Role::Ask) => {
            if let Err(e) = pipeline.start() {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Deactivated(Role::Ask) => {
            let pipeline = pipeline.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pipeline.stop_and_ask().await {
                    tracing::error!(%e, "ask failed");
                }
            });
        }
        Event::Activated(Role::Rewrite) => {
            // Read the selection now, before recording moves focus. The
            // thorough read: this key means the user asked for a rewrite, so
            // the clipboard fallback is worth its cost.
            pipeline.capture_selection();
            if let Err(e) = pipeline.start() {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Deactivated(Role::Rewrite) => {
            let pipeline = pipeline.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pipeline.stop_and_dictate().await {
                    tracing::error!(%e, "rewrite failed");
                }
            });
        }
        Event::Activated(Role::Agent) => {
            if let Err(e) = pipeline.start() {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Deactivated(Role::Agent) => {
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
        // Meeting notes is a one-shot toggle: start a session, or stop and
        // save the one in flight.
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
    // One guard across the whole toggle. Taking the lock, dropping it, and
    // re-taking it to store the new session let two toggles land in the gap:
    // the tray fires this on the main thread and the hotkey fires it on the
    // event-tap thread, so both could see None, both open a recorder, and the
    // second store would orphan the first with no way to stop it. Nothing here
    // awaits, so holding the guard costs nothing — `Pipeline::start` already
    // does it this way, which is why it has never had the same race.
    let mut slot = meeting.lock().unwrap();
    match slot.take() {
        Some(session) => {
            // Stop and save off-thread: summarizing is a network round-trip.
            tauri::async_runtime::spawn(async move {
                match session.stop().await {
                    Ok(path) => tracing::info!(note = %path.display(), "meeting note saved"),
                    Err(e) => tracing::error!(%e, "saving meeting note failed"),
                }
            });
        }
        None => {
            let settings_snapshot = settings.lock().map(|s| s.clone()).unwrap_or_default();
            match meeting::Meeting::start(app.clone(), settings_snapshot) {
                Ok(session) => *slot = Some(session),
                Err(e) => tracing::error!(%e, "starting meeting failed"),
            }
        }
    }
}
