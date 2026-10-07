//! Wiring `aloud_notes::journal` to the main window: list, search, write,
//! remove, and the capture that turns a spoken thought into an entry.
//! Orchestration only — where entries live and what a valid one is belongs to
//! the crate.

use crate::AppState;
use aloud_hotkey::model::Role;
use aloud_notes::{journal, Note, SearchResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};
use tauri_specta::Event;

type CmdResult<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

fn dir() -> CmdResult<PathBuf> {
    journal::directory().map_err(err)
}

/// An entry was written by something other than the view showing it: a voice
/// capture, from the Journal key, the menu bar or the record button. Carries
/// the path only, so the words never ride an event every window receives.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "journal-changed")]
pub struct JournalChanged {
    pub path: String,
}

/// Tells the windows a capture just landed. Best effort: the entry is on disk
/// either way, and the view rereads on focus.
pub fn announce(app: &AppHandle, path: &Path) {
    let changed = JournalChanged {
        path: path.to_string_lossy().into_owned(),
    };
    if let Err(e) = changed.emit(app) {
        tracing::warn!(%e, "could not announce a journal entry");
    }
}

/// The newest entries, for the timeline.
#[tauri::command]
#[specta::specta]
pub fn journal_entries(limit: u32) -> CmdResult<Vec<Note>> {
    let mut entries = journal::entries(&dir()?);
    entries.truncate(limit as usize);
    Ok(entries)
}

/// Every entry holding all of `query`'s words, best match first.
#[tauri::command]
#[specta::specta]
pub fn journal_search(query: String) -> CmdResult<Vec<SearchResult>> {
    Ok(journal::search(&query, &journal::entries(&dir()?)))
}

/// Writes a typed entry, or the edit to one (`path`). Returns it as stored, so
/// the view shows exactly what is on disk.
#[tauri::command]
#[specta::specta]
pub fn journal_save(text: String, path: Option<PathBuf>) -> CmdResult<Note> {
    let dir = dir()?;
    let saved = journal::save(&dir, path.as_deref(), &text, chrono::Utc::now()).map_err(err)?;
    journal::entries(&dir)
        .into_iter()
        .find(|entry| entry.path == saved)
        .ok_or_else(|| "the entry was saved but could not be read back".into())
}

#[tauri::command]
#[specta::specta]
pub fn journal_delete(path: PathBuf) -> CmdResult<()> {
    journal::delete(&dir()?, &path).map_err(err)
}

/// Starts a voice capture for the journal, or stops and keeps the one running.
/// Returns whether a capture is now recording. A dictation in progress is left
/// alone: one microphone, one session.
#[tauri::command]
#[specta::specta]
pub fn journal_capture(state: State<'_, AppState>) -> CmdResult<bool> {
    toggle(&state.pipeline)
}

/// The one toggle the record button and the menu bar share.
pub fn toggle(pipeline: &std::sync::Arc<crate::pipeline::Pipeline>) -> CmdResult<bool> {
    if pipeline.owns(Role::Journal) {
        let pipeline = pipeline.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = pipeline.stop_and_journal().await {
                tracing::error!(%e, "journal capture failed");
            }
        });
        return Ok(false);
    }
    if pipeline.is_recording() {
        return Err("Finish the dictation in progress first.".into());
    }
    pipeline.start(Role::Journal).map_err(err)?;
    Ok(true)
}

/// Whether a journal capture owns the microphone now, for a window that opens
/// in the middle of one.
#[tauri::command]
#[specta::specta]
pub fn journal_capturing(state: State<'_, AppState>) -> bool {
    state.pipeline.owns(Role::Journal)
}

/// Opens the journal folder in Finder or Explorer: the entries are plain files,
/// and seeing them there is how anyone checks that they are theirs.
#[tauri::command]
#[specta::specta]
pub fn journal_reveal(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let dir = dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}
