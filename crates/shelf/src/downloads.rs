//! Downloads: which files in `~/Downloads` are finished, and a watcher that
//! reports each as it completes — in any browser, because it watches the
//! folder, not the browser.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::watch::{self, Handle};

/// Extensions browsers give a file while it is still arriving: Chrome and
/// Edge `.crdownload`, Safari `.download` (a folder), Firefox `.part`,
/// Opera `.opdownload`, and the generic `.partial` / `.tmp`.
const PARTIAL: &[&str] = &[
    "crdownload",
    "download",
    "part",
    "partial",
    "opdownload",
    "tmp",
];

/// Not a finished download: hidden (a `.DS_Store`, Chrome's
/// `.com.google.Chrome.x1y2z3` scratch file) or still being written.
pub fn is_unfinished(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return true;
    };
    name.starts_with('.')
        || path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| PARTIAL.contains(&e.to_ascii_lowercase().as_str()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[serde(rename = "ShelfDownload")]
pub struct Download {
    pub name: String,
    pub path: String,
    #[specta(type = specta_typescript::Number)]
    pub size: u64,
    /// Unix seconds it last changed — when it finished, to the second.
    #[specta(type = specta_typescript::Number)]
    pub finished: i64,
}

impl Download {
    pub fn at(path: &Path) -> Self {
        Self {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: path.to_string_lossy().into_owned(),
            size: std::fs::metadata(path).map_or(0, |m| m.len()),
            finished: crate::mtime_secs(path),
        }
    }
}

/// `~/Downloads`.
pub fn dir() -> Option<PathBuf> {
    crate::home().map(|h| h.join("Downloads"))
}

/// The newest `limit` finished files, newest first. Folders are left out: a
/// folder in Downloads is something the user made, not something that
/// downloaded.
pub fn recent(dir: &Path, limit: usize) -> Vec<Download> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut all: Vec<Download> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| !is_unfinished(p))
        .map(|p| Download::at(&p))
        .collect();
    all.sort_by(|a, b| {
        b.finished
            .cmp(&a.finished)
            .then_with(|| b.name.cmp(&a.name))
    });
    all.truncate(limit);
    all
}

/// Calls `on` for each download that finishes in `dir`.
///
/// A file only counts if it is *new* in the folder (created or renamed into
/// place): that is what a browser's final rename is, and what a copy from
/// elsewhere is. A file edited in place is not a download.
pub fn watch(dir: &Path, on: impl Fn(Download) + Send + 'static) -> Result<Handle> {
    watch::settled(
        dir,
        |p| !is_unfinished(p),
        Duration::from_millis(800),
        move |path| {
            // A rename's other half, and a folder that matched by name, are not
            // files to announce.
            if path.is_file() {
                on(Download::at(&path));
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn partial_and_hidden_files_are_not_finished_downloads() {
        for name in [
            "movie.mp4.crdownload",
            "app.dmg.download",
            "page.zip.part",
            "Unconfirmed 123456.crdownload",
            ".com.google.Chrome.AbC123",
            ".DS_Store",
            "x.PART",
        ] {
            assert!(is_unfinished(Path::new(&format!("/d/{name}"))), "{name}");
        }
        for name in [
            "movie.mp4",
            "report (1).pdf",
            "download.txt",
            "partial-results.csv",
        ] {
            assert!(!is_unfinished(Path::new(&format!("/d/{name}"))), "{name}");
        }
    }

    #[test]
    fn recent_skips_partials_and_folders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("done.pdf"), "x").unwrap();
        std::fs::write(dir.path().join("busy.pdf.crdownload"), "x").unwrap();
        std::fs::create_dir(dir.path().join("a folder")).unwrap();
        std::fs::create_dir(dir.path().join("pending.dmg.download")).unwrap();
        let names: Vec<_> = recent(dir.path(), 10).into_iter().map(|d| d.name).collect();
        assert_eq!(names, ["done.pdf"]);
    }

    /// Real FSEvents: a browser writes `x.crdownload`, then renames it. The
    /// watcher must say nothing for the partial and exactly one thing for the
    /// finished file.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_browser_style_download_is_announced_once_when_it_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (tx, rx) = mpsc::channel();
        let _watch = watch(&root, move |d| tx.send(d.name).unwrap()).unwrap();
        std::thread::sleep(Duration::from_millis(500));

        let partial = root.join("big.zip.crdownload");
        std::fs::write(&partial, "first half").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(&partial, "first half, second half").unwrap();
        assert!(
            rx.recv_timeout(Duration::from_millis(1500)).is_err(),
            "partial announced"
        );

        std::fs::rename(&partial, root.join("big.zip")).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(6)).unwrap(), "big.zip");
        assert!(
            rx.recv_timeout(Duration::from_millis(1500)).is_err(),
            "announced twice"
        );
    }
}
