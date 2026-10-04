//! Screenshots: where macOS puts them, which files are one, and a watcher
//! that reports each as it lands.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::watch::{self, Handle};

/// Where screenshots go and what they are called — both are settings in
/// `com.apple.screencapture`, and both are commonly changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub dir: PathBuf,
    pub prefix: String,
}

const DEFAULT_PREFIX: &str = "Screenshot";
/// macOS before Ventura named them this.
const LEGACY_PREFIX: &str = "Screen Shot";

impl Location {
    /// Reads `defaults`; anything unset or gone is the default (the Desktop).
    pub fn read() -> Self {
        let get = |key: &str| {
            crate::run(
                "defaults",
                &[
                    "read".as_ref(),
                    "com.apple.screencapture".as_ref(),
                    key.as_ref(),
                ],
            )
            .ok()
        };
        Self::from_defaults(
            get("location").as_deref(),
            get("name").as_deref(),
            crate::home(),
        )
    }

    /// The pure half of [`Location::read`]: the two `defaults` outputs (None
    /// when the key is unset) and the home folder.
    pub fn from_defaults(
        location: Option<&str>,
        name: Option<&str>,
        home: Option<PathBuf>,
    ) -> Self {
        let home = home.unwrap_or_else(|| PathBuf::from("/"));
        let dir = location
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| match s.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                None if s == "~" => home.clone(),
                None => PathBuf::from(s),
            })
            // A location pointing at a folder that no longer exists makes
            // macOS fall back to the Desktop, so the watcher must too.
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| home.join("Desktop"));
        let prefix = name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_PREFIX)
            .to_string();
        Self { dir, prefix }
    }

    /// Is this file name one of ours — by prefix, a picture, not a hidden
    /// temp file? Screen recordings (`.mov`) are not screenshots.
    pub fn is_shot(&self, path: &Path) -> bool {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return false;
        };
        if name.starts_with('.') {
            return false;
        }
        let picture = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "heic" | "tiff" | "gif" | "pdf"
            )
        });
        picture && (name.starts_with(&self.prefix) || name.starts_with(LEGACY_PREFIX))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[serde(rename = "ScreenShot")]
pub struct Shot {
    pub name: String,
    pub path: String,
    /// Unix seconds.
    #[specta(type = specta_typescript::Number)]
    pub taken: i64,
}

impl Shot {
    pub fn at(path: &Path) -> Self {
        Self {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: path.to_string_lossy().into_owned(),
            taken: crate::mtime_secs(path),
        }
    }
}

/// The newest `limit` screenshots, newest first.
pub fn recent(loc: &Location, limit: usize) -> Vec<Shot> {
    let Ok(entries) = std::fs::read_dir(&loc.dir) else {
        return Vec::new();
    };
    let mut shots: Vec<Shot> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| loc.is_shot(p))
        .map(|p| Shot::at(&p))
        .collect();
    shots.sort_by(|a, b| b.taken.cmp(&a.taken).then_with(|| b.name.cmp(&a.name)));
    shots.truncate(limit);
    shots
}

/// Calls `on` for each new screenshot once it is fully written.
pub fn watch(loc: Location, on: impl Fn(Shot) + Send + 'static) -> Result<Handle> {
    let dir = loc.dir.clone();
    let on_path = move |path: PathBuf| {
        if loc.is_shot(&path) {
            on(Shot::at(&path));
        }
    };
    // Name filtering happens in `on_path` (it knows the prefix); the watcher
    // only has to skip hidden temp files before waiting on them.
    watch::settled(&dir, |p| !is_hidden(p), Duration::from_millis(600), on_path)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_none_or(|n| n.starts_with('.'))
}

/// How `screencapture` is asked to capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShotCapture {
    /// Drag a region; the picture lands in the screenshot folder, so the
    /// shelf's watcher picks it up like any other.
    Region,
    /// Drag a region; the picture goes to the clipboard only.
    Clipboard,
}

/// Starts `screencapture`'s interactive crosshair. Returns at once: the user
/// is the one who finishes it, and Escape cancels with no file. The process
/// is reaped in the background so it does not linger as a zombie.
pub fn capture(loc: &Location, mode: ShotCapture) -> Result<()> {
    let mut cmd = std::process::Command::new("/usr/sbin/screencapture");
    match mode {
        ShotCapture::Clipboard => {
            cmd.args(["-i", "-c"]);
        }
        ShotCapture::Region => {
            let stamp = crate::mtime_secs_now();
            cmd.arg("-i")
                .arg(loc.dir.join(format!("{} {stamp}.png", loc.prefix)));
        }
    }
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Puts a screenshot on the clipboard as a picture (what pasting into Slack
/// or Mail wants), through AppleScript's own clipboard so the pixels are not
/// decoded here. The path goes in as an argument, never into the script text.
pub fn copy(path: &Path) -> Result<()> {
    let class = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "JPEG",
        Some("tiff") => "TIFF",
        Some("gif") => "GIFf",
        // A PDF or HEIC has no clipboard class we can read it as; copy the
        // file itself so a paste still gives the user their screenshot.
        Some("png") => "PNGf",
        _ => "furl",
    };
    let script = if class == "furl" {
        "on run argv\nset the clipboard to (POSIX file (item 1 of argv))\nend run".to_string()
    } else {
        format!("on run argv\nset the clipboard to (read (POSIX file (item 1 of argv)) as «class {class}»)\nend run")
    };
    crate::run(
        "osascript",
        &["-e".as_ref(), script.as_ref(), path.as_os_str()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(prefix: &str) -> Location {
        Location {
            dir: "/x".into(),
            prefix: prefix.into(),
        }
    }

    #[test]
    fn recognises_both_eras_of_macos_names_and_a_custom_prefix() {
        let l = loc("Screenshot");
        assert!(l.is_shot(Path::new("/x/Screenshot 2026-10-04 at 9.41.02 AM.png")));
        assert!(l.is_shot(Path::new("/x/Screen Shot 2019-01-02 at 1.00.00 PM.png")));
        assert!(loc("Shot").is_shot(Path::new("/x/Shot 12.png")));
        assert!(!loc("Shot").is_shot(Path::new("/x/Screenshot 12.png")));
    }

    #[test]
    fn ignores_recordings_hidden_temp_files_and_other_pictures() {
        let l = loc("Screenshot");
        assert!(!l.is_shot(Path::new("/x/Screen Recording 2026.mov")));
        assert!(!l.is_shot(Path::new("/x/.Screenshot 1.png")));
        assert!(!l.is_shot(Path::new("/x/holiday.png")));
        assert!(!l.is_shot(Path::new("/x/Screenshot notes.txt")));
    }

    #[test]
    fn location_expands_tilde_and_falls_back_to_the_desktop_when_the_folder_is_gone() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("Pics")).unwrap();
        let l = Location::from_defaults(Some("~/Pics\n"), Some("Snap\n"), Some(home.path().into()));
        assert_eq!(l.dir, home.path().join("Pics"));
        assert_eq!(l.prefix, "Snap");

        let gone = Location::from_defaults(Some("~/Deleted\n"), None, Some(home.path().into()));
        assert_eq!(gone.dir, home.path().join("Desktop"));
        assert_eq!(gone.prefix, "Screenshot");

        let unset = Location::from_defaults(None, None, Some(home.path().into()));
        assert_eq!(unset.dir, home.path().join("Desktop"));
    }

    #[test]
    fn recent_is_newest_first_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        for (i, name) in [
            "Screenshot a.png",
            "Screenshot b.png",
            "Screenshot c.png",
            "other.png",
        ]
        .iter()
        .enumerate()
        {
            let p = dir.path().join(name);
            std::fs::write(&p, "x").unwrap();
            let t = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_000 + i as u64 * 10);
            std::fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        let l = Location {
            dir: dir.path().into(),
            prefix: "Screenshot".into(),
        };
        let got: Vec<_> = recent(&l, 2).into_iter().map(|s| s.name).collect();
        assert_eq!(got, ["Screenshot c.png", "Screenshot b.png"]);
    }
}
