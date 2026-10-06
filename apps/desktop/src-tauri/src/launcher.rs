//! Wiring `aloud-launcher` to the notch's Launch section. Orchestration only:
//! the pins and recents rules, the `shortcuts` and `mdfind` calls are the
//! crate's.

use aloud_launcher::{LaunchFile, LauncherPrefs, PrefsStore};
use std::path::Path;
use tauri::{AppHandle, Manager, State};

type CmdResult<T> = Result<T, String>;

pub struct LauncherState(PrefsStore);

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// Loads pins and recents. Called once from `setup`.
pub fn start(app: &AppHandle) {
    // Pins are not sensitive, so a missing data folder falls back to the
    // temp dir rather than a launcher that cannot remember anything.
    let dir = aloud_notes::data_dir().unwrap_or_else(|_| std::env::temp_dir());
    app.manage(LauncherState(PrefsStore::open(&dir)));
}

#[tauri::command]
#[specta::specta]
pub fn launcher_prefs(state: State<'_, LauncherState>) -> LauncherPrefs {
    state.0.get()
}

/// Opens an app and moves it to the front of the recents. The panel closes:
/// the app is about to take the screen.
#[tauri::command]
#[specta::specta]
pub async fn launcher_open_app(
    app: AppHandle,
    state: State<'_, LauncherState>,
    name: String,
) -> CmdResult<LauncherPrefs> {
    let opening = name.clone();
    tauri::async_runtime::spawn_blocking(move || aloud_launcher::open_app(&opening))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)?;
    crate::overlay::end_panel(&app);
    state.0.update(|p| p.touch(&name)).map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn launcher_pin(
    state: State<'_, LauncherState>,
    name: String,
    pinned: bool,
) -> CmdResult<LauncherPrefs> {
    state.0.update(|p| p.set_pin(&name, pinned)).map_err(err)
}

#[tauri::command]
#[specta::specta]
pub async fn shortcut_list() -> CmdResult<Vec<String>> {
    tauri::async_runtime::spawn_blocking(aloud_launcher::list_shortcuts)
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

/// Runs a shortcut and answers when it finishes. A click in the panel is the
/// user's own decision to run it, so there is no approval card.
#[tauri::command]
#[specta::specta]
pub async fn shortcut_run(name: String) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || aloud_launcher::run_shortcut(&name))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[tauri::command]
#[specta::specta]
pub async fn launcher_files(query: String) -> CmdResult<Vec<LaunchFile>> {
    tauri::async_runtime::spawn_blocking(move || aloud_launcher::search_files(&query, 6))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[tauri::command]
#[specta::specta]
pub async fn launcher_recent_files() -> CmdResult<Vec<LaunchFile>> {
    tauri::async_runtime::spawn_blocking(|| aloud_launcher::recent_files(6))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

/// Opens a file or a note in its default app.
#[tauri::command]
#[specta::specta]
pub async fn launcher_open_path(app: AppHandle, path: String) -> CmdResult<()> {
    let target = Path::new(&path);
    // Absolute, so `open` can never read it as an option.
    if !target.is_absolute() || !target.exists() {
        return Err("that file is gone".into());
    }
    let status = tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new("open").arg(path).status()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("could not open that file".into());
    }
    crate::overlay::end_panel(&app);
    Ok(())
}
