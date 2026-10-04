//! Wiring the Shelf (`aloud-shelf`) to the notch: its commands, the watchers
//! behind each on/off row, and the islands they push. What a screenshot is,
//! when a download has finished and which volume is a drive are the crate's;
//! this file only connects them to Tauri.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use aloud_islands::{Island, IslandAction, IslandKind, IslandPriority};
use aloud_shelf::downloads::{self, Download};
use aloud_shelf::drives::{self, Change, Drive};
use aloud_shelf::files::{Item, Shelf, ShelfImageFormat};
use aloud_shelf::prefs::{Prefs, ShelfWatch};
use aloud_shelf::shots::{self, Location, Shot, ShotCapture};
use aloud_shelf::{thumb, watch::Handle};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

type CmdResult<T> = Result<T, String>;

const LABEL: &str = "overlay";
/// Thumbnails are drawn at 30px; twice that is sharp on a Retina panel.
const THUMB_PX: u32 = 64;
/// How long a Shelf alert may sit in the queue if the webview never clears
/// it — the chip's own dwell is the webview's four seconds.
const ALERT_BACKSTOP_MS: u32 = 30_000;

/// Which list changed, so the view showing it can read it again.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShelfArea {
    Files,
    Shots,
    Downloads,
    Drives,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "shelf-changed")]
pub struct ShelfChanged(pub ShelfArea);

fn changed(app: &AppHandle, area: ShelfArea) {
    if let Err(e) = ShelfChanged(area).emit_to(app, LABEL) {
        tracing::warn!(%e, "could not send shelf change");
    }
}

#[derive(Default)]
struct Watchers {
    shots: Option<Handle>,
    downloads: Option<Handle>,
    drives: Option<Handle>,
}

pub struct ShelfState {
    shelf: Shelf,
    dir: PathBuf,
    prefs: Mutex<Prefs>,
    watchers: Mutex<Watchers>,
}

impl ShelfState {
    fn thumbs(&self) -> PathBuf {
        self.dir.join("thumbs")
    }

    fn prefs_file(&self) -> PathBuf {
        self.dir.join("prefs.json")
    }
}

/// Opens the shelf and starts the watchers the user left on. Called from
/// `setup`; nothing here blocks it — a watcher is a thread.
pub fn init(app: &AppHandle) {
    let dir = aloud_notes::data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("Aloud"))
        .join("shelf");
    let prefs = Prefs::load(&dir.join("prefs.json"));
    app.manage(ShelfState {
        shelf: Shelf::open(dir.clone()),
        dir,
        prefs: Mutex::new(prefs),
        watchers: Mutex::default(),
    });
    for (which, on) in [
        (ShelfWatch::Shots, prefs.shots),
        (ShelfWatch::Downloads, prefs.downloads),
        (ShelfWatch::Drives, prefs.drives),
    ] {
        if on {
            if let Err(e) = start(app, which) {
                tracing::warn!(?which, %e, "shelf watcher did not start");
            }
        }
    }
}

fn start(app: &AppHandle, which: ShelfWatch) -> anyhow::Result<()> {
    let state = app.state::<ShelfState>();
    let mut watchers = state
        .watchers
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let handle = {
        let app = app.clone();
        match which {
            ShelfWatch::Shots => shots::watch(Location::read(), move |shot| on_shot(&app, shot))?,
            ShelfWatch::Downloads => {
                let dir = downloads::dir().ok_or_else(|| anyhow::anyhow!("no home folder"))?;
                downloads::watch(&dir, move |d| on_download(&app, d))?
            }
            ShelfWatch::Drives => drives::watch(move |c| on_drive(&app, c))?,
        }
    };
    // Dropping the old handle (a restart after the screenshot folder moved)
    // stops it.
    match which {
        ShelfWatch::Shots => watchers.shots = Some(handle),
        ShelfWatch::Downloads => watchers.downloads = Some(handle),
        ShelfWatch::Drives => watchers.drives = Some(handle),
    }
    Ok(())
}

fn stop(app: &AppHandle, which: ShelfWatch) {
    let state = app.state::<ShelfState>();
    let mut watchers = state
        .watchers
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    match which {
        ShelfWatch::Shots => watchers.shots = None,
        ShelfWatch::Downloads => watchers.downloads = None,
        ShelfWatch::Drives => watchers.drives = None,
    }
}

// ---- islands ---------------------------------------------------------------

fn alert(
    title: String,
    detail: &str,
    icon: Option<String>,
    actions: Vec<IslandAction>,
    id: String,
) -> Island {
    Island {
        id,
        kind: IslandKind::Alert,
        priority: IslandPriority::Normal,
        icon,
        title,
        detail: Some(detail.to_string()),
        actions,
        expires: Some(ALERT_BACKSTOP_MS),
    }
}

fn shot_island(shot: &Shot, thumb: Option<String>) -> Island {
    alert(
        "Screenshot saved".into(),
        &shot.name,
        // The picture itself is the icon: a screenshot has no app to borrow one from.
        thumb,
        vec![
            IslandAction::new("Copy", "shelf.copyShot").arg("path", &shot.path),
            IslandAction::new("Show", "shelf.reveal").arg("path", &shot.path),
        ],
        format!("shot:{}", shot.path),
    )
}

fn download_island(d: &Download) -> Island {
    alert(
        format!("Downloaded {}", d.name),
        "Downloads",
        None,
        vec![IslandAction::new("Show", "shelf.reveal").arg("path", &d.path)],
        format!("download:{}", d.path),
    )
}

fn drive_island(d: &Drive) -> Island {
    alert(
        format!("{} connected", d.name),
        &d.bus,
        None,
        vec![IslandAction::new("Eject", "shelf.eject").arg("mount", &d.mount)],
        format!("drive:{}", d.mount),
    )
}

fn ejected_island(name: &str, mount: &str) -> Island {
    alert(
        format!("{name} is safe to disconnect"),
        "Disk",
        None,
        Vec::new(),
        format!("drive:{mount}"),
    )
}

/// A Shelf alert drops from the notch — unless the user is in a panel, which
/// it must not take the window from; the list in Shelf still gets it.
fn show(app: &AppHandle, island: Island) {
    if crate::overlay::show_toast(app, false) {
        crate::islands::push(app, island);
    }
}

fn on_shot(app: &AppHandle, shot: Shot) {
    let thumb = {
        let state = app.state::<ShelfState>();
        thumb::data_uri(&state.thumbs(), Path::new(&shot.path), THUMB_PX).ok()
    };
    changed(app, ShelfArea::Shots);
    show(app, shot_island(&shot, thumb));
}

fn on_download(app: &AppHandle, d: Download) {
    changed(app, ShelfArea::Downloads);
    show(app, download_island(&d));
}

fn on_drive(app: &AppHandle, change: Change) {
    changed(app, ShelfArea::Drives);
    match change {
        Change::Added(d) => show(app, drive_island(&d)),
        // Pulled out or ejected: either way the Eject button has nothing left
        // to eject. "Safe to disconnect" comes only from our own eject, never
        // from here — a yanked cable is the opposite of safe.
        Change::Removed(d) => crate::islands::remove(app, &format!("drive:{}", d.mount)),
    }
}

// ---- commands --------------------------------------------------------------

/// Runs a blocking Shelf job off the async runtime: all of these spawn macOS
/// tools.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    job: impl FnOnce(&ShelfState) -> T + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(move || job(&app.state::<ShelfState>()))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn shelf_items(state: tauri::State<'_, ShelfState>) -> Vec<Item> {
    state.shelf.items()
}

/// What a drop put on the shelf. `failed` names what could not be copied;
/// the rest are shelved.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShelfAdded {
    pub added: Vec<Item>,
    pub failed: Vec<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn shelf_add(app: AppHandle, paths: Vec<String>) -> CmdResult<ShelfAdded> {
    let (added, failed) = blocking(app.clone(), move |s| {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        s.shelf.add(&paths)
    })
    .await?;
    changed(&app, ShelfArea::Files);
    Ok(ShelfAdded { added, failed })
}

#[tauri::command]
#[specta::specta]
pub async fn shelf_remove(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app.clone(), move |s| {
        s.shelf.remove(&id).map_err(|e| e.to_string())
    })
    .await??;
    changed(&app, ShelfArea::Files);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn shelf_clear(app: AppHandle) -> CmdResult<()> {
    blocking(app.clone(), |s| s.shelf.clear().map_err(|e| e.to_string())).await??;
    changed(&app, ShelfArea::Files);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn shelf_convert(app: AppHandle, id: String, to: ShelfImageFormat) -> CmdResult<Item> {
    let item = blocking(app.clone(), move |s| {
        s.shelf.convert(&id, to).map_err(|e| format!("{e:#}"))
    })
    .await??;
    changed(&app, ShelfArea::Files);
    Ok(item)
}

#[tauri::command]
#[specta::specta]
pub async fn shelf_zip(app: AppHandle, ids: Vec<String>) -> CmdResult<Item> {
    let item = blocking(app.clone(), move |s| {
        s.shelf.zip(&ids).map_err(|e| format!("{e:#}"))
    })
    .await??;
    changed(&app, ShelfArea::Files);
    Ok(item)
}

#[tauri::command]
#[specta::specta]
pub async fn shelf_unzip(app: AppHandle, id: String) -> CmdResult<Item> {
    let item = blocking(app.clone(), move |s| {
        s.shelf.unzip(&id).map_err(|e| format!("{e:#}"))
    })
    .await??;
    changed(&app, ShelfArea::Files);
    Ok(item)
}

/// Opens macOS's AirDrop picker for shelved items. AppKit sharing wants the
/// main thread, so the job hops there and reports back.
#[tauri::command]
#[specta::specta]
pub async fn shelf_airdrop(app: AppHandle, ids: Vec<String>) -> CmdResult<()> {
    let paths = app
        .state::<ShelfState>()
        .shelf
        .paths(&ids)
        .map_err(|e| e.to_string())?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(aloud_shelf::airdrop::share(&paths).map_err(|e| format!("{e:#}")));
    })
    .map_err(|e| e.to_string())?;
    rx.await.map_err(|e| e.to_string())?
}

/// A `data:` thumbnail of a picture the Shelf shows — and only those: the
/// path must sit in the shelf, the screenshot folder or Downloads.
#[tauri::command]
#[specta::specta]
pub async fn shelf_thumbnail(app: AppHandle, path: String) -> CmdResult<Option<String>> {
    blocking(app, move |s| {
        let mut roots = vec![s.dir.clone()];
        roots.push(Location::read().dir);
        roots.extend(downloads::dir());
        let path = PathBuf::from(path);
        if !thumb::within(&path, &roots) {
            return None;
        }
        thumb::data_uri(&s.thumbs(), &path, THUMB_PX).ok()
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn shelf_prefs(state: tauri::State<'_, ShelfState>) -> Prefs {
    *state.prefs.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Turns a watcher on or off and remembers it. A watcher that cannot start
/// (a folder macOS has not allowed yet) leaves the switch off and says why.
#[tauri::command]
#[specta::specta]
pub async fn shelf_set_watch(app: AppHandle, which: ShelfWatch, on: bool) -> CmdResult<Prefs> {
    let started = if on {
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || start(&app, which))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:#}"))
    } else {
        stop(&app, which);
        Ok(())
    };
    let state = app.state::<ShelfState>();
    let mut prefs = state.prefs.lock().unwrap_or_else(PoisonError::into_inner);
    prefs.set(which, on && started.is_ok());
    if let Err(e) = prefs.save(&state.prefs_file()) {
        tracing::warn!(%e, "could not save shelf settings");
    }
    let now = *prefs;
    drop(prefs);
    started.map(|()| now)
}

#[tauri::command]
#[specta::specta]
pub async fn shots_recent(app: AppHandle, limit: u32) -> CmdResult<Vec<Shot>> {
    blocking(app, move |_| {
        shots::recent(&Location::read(), limit as usize)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn shots_copy(app: AppHandle, path: String) -> CmdResult<()> {
    blocking(app, move |_| {
        let path = PathBuf::from(path);
        // Only a screenshot: this reaches the clipboard, which is where
        // secrets pass through.
        let loc = Location::read();
        if !loc.is_shot(&path) {
            return Err("that is not a screenshot".to_string());
        }
        shots::copy(&path).map_err(|e| format!("{e:#}"))
    })
    .await?
}

/// Starts macOS's region capture. It returns at once; the user finishes it.
#[tauri::command]
#[specta::specta]
pub async fn shots_capture(app: AppHandle, mode: ShotCapture) -> CmdResult<()> {
    blocking(app, move |_| {
        shots::capture(&Location::read(), mode).map_err(|e| format!("{e:#}"))
    })
    .await?
}

#[tauri::command]
#[specta::specta]
pub async fn downloads_recent(app: AppHandle, limit: u32) -> CmdResult<Vec<Download>> {
    blocking(app, move |_| {
        downloads::dir()
            .map(|d| downloads::recent(&d, limit as usize))
            .unwrap_or_default()
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn drives_list(app: AppHandle) -> CmdResult<Vec<Drive>> {
    blocking(app, |_| drives::list()).await
}

/// Ejects, then says it is safe — the only place that is said. A drive in
/// use fails with whatever is holding it, in diskutil's words.
#[tauri::command]
#[specta::specta]
pub async fn drive_eject(app: AppHandle, mount: String) -> CmdResult<()> {
    let name = Path::new(&mount)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let target = mount.clone();
    blocking(app.clone(), move |_| {
        drives::eject(Path::new(&target)).map_err(|e| format!("{e:#}"))
    })
    .await??;
    changed(&app, ShelfArea::Drives);
    show(&app, ejected_island(&name, &mount));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drives_connected_alert_is_replaced_by_its_safe_to_disconnect_alert() {
        let d = Drive {
            name: "Backup".into(),
            mount: "/Volumes/Backup".into(),
            bus: "USB".into(),
            size: 1,
            free: 1,
        };
        let connected = drive_island(&d);
        let safe = ejected_island("Backup", "/Volumes/Backup");
        // Same id: the queue updates one chip in place instead of stacking a
        // stale Eject button beneath the confirmation.
        assert_eq!(connected.id, safe.id);
        assert_eq!(connected.actions[0].action, "shelf.eject");
        assert!(safe.actions.is_empty());
    }

    #[test]
    fn island_buttons_name_registered_actions_with_the_args_they_need() {
        let shot = Shot {
            name: "Screenshot a.png".into(),
            path: "/d/Screenshot a.png".into(),
            taken: 0,
        };
        let island = shot_island(&shot, None);
        let names: Vec<_> = island.actions.iter().map(|a| a.action.as_str()).collect();
        assert_eq!(names, ["shelf.copyShot", "shelf.reveal"]);
        assert!(island
            .actions
            .iter()
            .all(|a| a.args.get("path") == Some(&shot.path)));
    }
}
