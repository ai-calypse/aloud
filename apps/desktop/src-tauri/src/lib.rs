mod ask;
mod commands;
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
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

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
            app.manage(AppState {
                settings: settings.clone(),
                pipeline: pipeline.clone(),
            });

            tray::init(app)?;
            spawn_hotkey_listener(settings, pipeline);

            // Menu-bar app: no dock icon, no taskbar entry.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Runs the key hook on its own thread: both backends block forever (a run
/// loop on macOS, a message pump on Windows).
fn spawn_hotkey_listener(settings: Arc<Mutex<Settings>>, pipeline: Arc<pipeline::Pipeline>) {
    if !aloud_hotkey::is_permitted() {
        tracing::warn!("accessibility permission not granted; hotkeys are disabled");
        return;
    }

    // The hotkey config is a slice of settings; keep them in sync so a
    // settings change applies without restarting the hook.
    let config = Arc::new(Mutex::new(
        settings
            .lock()
            .map(|s| s.hotkeys.clone())
            .unwrap_or_default(),
    ));

    std::thread::spawn(move || {
        let result = aloud_hotkey::listen(config, move |event| {
            handle_hotkey(&pipeline, event);
        });
        if let Err(e) = result {
            tracing::error!(%e, "hotkey listener stopped");
        }
    });
}

fn handle_hotkey(pipeline: &Arc<pipeline::Pipeline>, event: aloud_hotkey::model::Event) {
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
        Event::Activated(Role::CopyAgain) => {
            if let Err(e) = pipeline.paste_again() {
                tracing::error!(%e, "could not paste again");
            }
        }
        Event::Cancelled => pipeline.cancel(),
        other => tracing::debug!(?other, "unhandled hotkey event"),
    }
}
