mod agent;
mod ask;
mod commands;
mod meeting;
mod overlay;
mod pipeline;
mod tray;

use aloud_store::Settings;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri_specta::{collect_commands, Builder};

/// Shared app state, injected into commands via `tauri::State`.
pub struct AppState {
    pub settings: Arc<Mutex<Settings>>,
    pub pipeline: Arc<pipeline::Pipeline>,
    /// The running meeting session, if any. A one-shot hotkey toggles it.
    pub meeting: Arc<Mutex<Option<meeting::Meeting>>>,
    /// The live hotkey config the listener reads. Shared with the listener
    /// thread so a rebind from Settings applies without a restart.
    pub hotkey_config: Arc<Mutex<aloud_hotkey::model::Config>>,
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "aloud=info,aloud_hotkey=info".into()),
        )
        .init();

    let builder = Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::app_version,
        commands::get_settings,
        commands::update_settings,
        commands::get_stats,
        commands::get_history,
        commands::search_notes,
        commands::recent_notes,
        commands::input_devices,
        commands::hotkeys_permitted,
        commands::paste_again,
        ask::close_ask,
        agent::confirm_agent,
        agent::close_agent,
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
        .plugin(tauri_plugin_opener::init())
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
            let meeting: Arc<Mutex<Option<meeting::Meeting>>> = Arc::new(Mutex::new(None));
            // The live hotkey config, shared between AppState (written on a
            // rebind) and the listener thread (which reads it every event).
            let hotkey_config = Arc::new(Mutex::new(
                settings.lock().map(|s| s.hotkeys.clone()).unwrap_or_default(),
            ));
            app.manage(AppState {
                settings: settings.clone(),
                pipeline: pipeline.clone(),
                meeting: meeting.clone(),
                hotkey_config: hotkey_config.clone(),
            });
            app.manage(agent::PendingConfirmation::default());

            tray::init(app)?;
            spawn_hotkey_listener(
                app.handle().clone(),
                settings,
                pipeline,
                meeting,
                hotkey_config,
            );

            // Closing the window must hide it, not destroy it. This is a
            // menu-bar app: the window is reopened from the tray, and a
            // destroyed window makes `get_webview_window("main")` return None,
            // so "Open Aloud" silently does nothing. Hide and prevent the close.
            if let Some(main) = app.get_webview_window("main") {
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

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
) {
    if !aloud_hotkey::is_permitted() {
        tracing::warn!("accessibility permission not granted; hotkeys are disabled");
        return;
    }

    // `config` is the same Arc AppState holds, so a rebind from Settings is
    // seen here on the next key event — no restart needed.
    let settings = settings.clone();
    std::thread::spawn(move || {
        let result = aloud_hotkey::listen(config, move |event| {
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
            if let Err(e) = pipeline.start() {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Deactivated(Role::Hold) => {
            // Transcription is async and the hook thread must not block: a
            // stalled hook stops delivering key events system-wide.
            let pipeline = pipeline.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pipeline.stop_and_paste().await {
                    tracing::error!(%e, "dictation failed");
                }
            });
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
        Event::Activated(Role::Command) => {
            // Read the selection now, before recording moves focus.
            pipeline.capture_selection();
            if let Err(e) = pipeline.start() {
                tracing::error!(%e, "could not start recording");
            }
        }
        Event::Deactivated(Role::Command) => {
            let pipeline = pipeline.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pipeline.stop_and_command().await {
                    tracing::error!(%e, "command mode failed");
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
fn toggle_meeting(
    app: &tauri::AppHandle,
    settings: &Arc<Mutex<Settings>>,
    meeting: &Arc<Mutex<Option<meeting::Meeting>>>,
) {
    let running = meeting.lock().unwrap().take();
    match running {
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
                Ok(session) => *meeting.lock().unwrap() = Some(session),
                Err(e) => tracing::error!(%e, "starting meeting failed"),
            }
        }
    }
}
