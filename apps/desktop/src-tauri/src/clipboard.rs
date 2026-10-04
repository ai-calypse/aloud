//! Wiring `aloud-clipboard` to the notch's Clipboard section: the store as
//! managed state, the watcher's `clipboard-changed` event, and the commands.
//! Orchestration only — what is kept, deduped and offered is the crate's.

use std::sync::Arc;
use std::time::Duration;

use aloud_clipboard::{ClipboardEntry, PickedColor, Snippet, Store};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

type CmdResult<T> = Result<T, String>;

/// `None` when the store could not be opened; every command then says so
/// rather than silently keeping nothing.
pub struct ClipboardStore(pub Option<Arc<Store>>);

/// The history changed (a copy was recorded), so an open panel can refresh.
/// Carries no content: the event reaches every listener.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "clipboard-changed")]
pub struct ClipboardChanged;

/// Time for the panel to close and the app behind it to take key focus
/// before a synthetic paste goes to it.
const FOCUS_SETTLE: Duration = Duration::from_millis(250);

fn store(state: &State<'_, ClipboardStore>) -> CmdResult<Arc<Store>> {
    state
        .0
        .clone()
        .ok_or_else(|| "clipboard storage is unavailable".to_string())
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// Opens the store and starts the watcher. Called once from `setup`.
pub fn start(app: &AppHandle) {
    let opened = aloud_notes::data_dir()
        .and_then(|dir| Store::open(&dir.join("clipboard")))
        .map(Arc::new);
    match &opened {
        Ok(store) => {
            let handle = app.clone();
            aloud_clipboard::watch::spawn(store.clone(), move || {
                let _ = ClipboardChanged.emit_to(&handle, "overlay");
            });
        }
        Err(e) => tracing::warn!(error = %format!("{e:#}"), "clipboard history unavailable"),
    }
    app.manage(ClipboardStore(opened.ok()));
}

/// Hands focus back before a paste: the panel is the key window while open.
fn paste_soon(app: &AppHandle, then: impl FnOnce() + Send + 'static) {
    crate::overlay::end_panel(app);
    std::thread::spawn(move || {
        std::thread::sleep(FOCUS_SETTLE);
        then();
    });
}

#[tauri::command]
#[specta::specta]
pub fn clipboard_enabled(state: State<'_, ClipboardStore>) -> bool {
    state.0.as_ref().is_some_and(|s| s.enabled())
}

#[tauri::command]
#[specta::specta]
pub fn clipboard_set_enabled(state: State<'_, ClipboardStore>, on: bool) -> CmdResult<()> {
    store(&state)?.set_enabled(on).map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn clipboard_list(
    state: State<'_, ClipboardStore>,
    query: String,
) -> CmdResult<Vec<ClipboardEntry>> {
    store(&state)?.list(&query, 100).map_err(err)
}

#[tauri::command]
#[specta::specta]
pub async fn clipboard_copy(state: State<'_, ClipboardStore>, id: u32) -> CmdResult<()> {
    let store = store(&state)?;
    tauri::async_runtime::spawn_blocking(move || aloud_clipboard::actions::copy(&store, id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

/// Copies an entry and pastes it into the app behind the panel.
#[tauri::command]
#[specta::specta]
pub async fn clipboard_paste(
    app: AppHandle,
    state: State<'_, ClipboardStore>,
    id: u32,
) -> CmdResult<()> {
    let store = store(&state)?;
    tauri::async_runtime::spawn_blocking(move || aloud_clipboard::actions::copy(&store, id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)?;
    paste_soon(&app, || {
        if let Err(e) = aloud_paste::paste_clipboard() {
            tracing::warn!(error = %format!("{e:#}"), "could not paste a history entry");
        }
    });
    Ok(())
}

/// Runs one of an entry's smart actions; the answer is a few words for the row.
#[tauri::command]
#[specta::specta]
pub async fn clipboard_action(
    state: State<'_, ClipboardStore>,
    id: u32,
    action: String,
) -> CmdResult<String> {
    let store = store(&state)?;
    tauri::async_runtime::spawn_blocking(move || aloud_clipboard::actions::run(&store, id, &action))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn clipboard_pin(state: State<'_, ClipboardStore>, id: u32, pinned: bool) -> CmdResult<()> {
    store(&state)?.pin(id, pinned).map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn clipboard_delete(state: State<'_, ClipboardStore>, id: u32) -> CmdResult<()> {
    store(&state)?.delete(id).map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn clipboard_clear(state: State<'_, ClipboardStore>) -> CmdResult<()> {
    store(&state)?.clear().map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn snippet_list(state: State<'_, ClipboardStore>) -> CmdResult<Vec<Snippet>> {
    store(&state)?.snippets().map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn snippet_save(
    state: State<'_, ClipboardStore>,
    id: Option<u32>,
    name: String,
    trigger: String,
    body: String,
) -> CmdResult<u32> {
    store(&state)?
        .save_snippet(id, &name, &trigger, &body)
        .map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn snippet_delete(state: State<'_, ClipboardStore>, id: u32) -> CmdResult<()> {
    store(&state)?.delete_snippet(id).map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn snippet_copy(state: State<'_, ClipboardStore>, id: u32) -> CmdResult<()> {
    let body = store(&state)?
        .snippet_body(id)
        .map_err(err)?
        .ok_or("that snippet no longer exists")?;
    aloud_paste::set_clipboard(&body).map_err(err)
}

/// Pastes a snippet into the app behind the panel, putting the user's own
/// clipboard back afterwards.
#[tauri::command]
#[specta::specta]
pub fn snippet_paste(app: AppHandle, state: State<'_, ClipboardStore>, id: u32) -> CmdResult<()> {
    let body = store(&state)?
        .snippet_body(id)
        .map_err(err)?
        .ok_or("that snippet no longer exists")?;
    paste_soon(&app, move || {
        if let Err(e) = aloud_paste::paste_text(&body, false, true) {
            tracing::warn!(error = %format!("{e:#}"), "could not paste a snippet");
        }
    });
    Ok(())
}

/// Shows the system eyedropper; the picked colour is saved and its hex copied.
/// None when the user cancelled.
#[tauri::command]
#[specta::specta]
pub async fn color_pick(
    app: AppHandle,
    state: State<'_, ClipboardStore>,
) -> CmdResult<Option<String>> {
    let store = store(&state)?;
    let picked = tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Option<String>> {
        let Some(rgb) = aloud_clipboard::colors::pick()? else {
            return Ok(None);
        };
        let hex = rgb.hex();
        store.record_color(&hex)?;
        aloud_paste::set_clipboard(&hex)?;
        Ok(Some(hex))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)?;
    if let Some(hex) = &picked {
        // The sampler takes focus from the panel, so the answer is a toast:
        // what was copied, where the user is looking.
        if crate::overlay::show_toast(&app, false) {
            crate::islands::push(
                &app,
                aloud_islands::Island {
                    id: "clipboard:color".into(),
                    kind: aloud_islands::IslandKind::Alert,
                    priority: aloud_islands::IslandPriority::Normal,
                    icon: None,
                    title: format!("Copied {hex}"),
                    detail: Some("Color picker".into()),
                    actions: Vec::new(),
                    expires: Some(30_000),
                },
            );
        }
    }
    Ok(picked)
}

#[tauri::command]
#[specta::specta]
pub fn color_list(state: State<'_, ClipboardStore>) -> CmdResult<Vec<PickedColor>> {
    store(&state)?.colors().map_err(err)
}

/// Copies a picked colour as `hex` or `rgb`.
#[tauri::command]
#[specta::specta]
pub fn color_copy(hex: String, format: String) -> CmdResult<()> {
    let rgb = aloud_clipboard::colors::parse(&hex).ok_or("not a colour")?;
    aloud_paste::set_clipboard(&if format == "rgb" {
        rgb.css()
    } else {
        rgb.hex()
    })
    .map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn color_clear(state: State<'_, ClipboardStore>) -> CmdResult<()> {
    store(&state)?.clear_colors().map_err(err)
}
