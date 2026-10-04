use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, PoisonError};

/// Recents kept. A launcher shows a row of them; more is a list to scroll.
const MAX_RECENTS: usize = 12;

/// App names, as `installed_apps` reports them (what `open -a` takes).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct LauncherPrefs {
    /// In the order the user pinned them.
    pub pins: Vec<String>,
    /// Most recent first.
    pub recents: Vec<String>,
}

impl LauncherPrefs {
    /// Pinning an already-pinned app is a no-op, so a double click cannot
    /// double a tile; unpinning one that is not pinned likewise.
    pub fn set_pin(&mut self, name: &str, pinned: bool) {
        self.pins.retain(|p| p != name);
        if pinned {
            self.pins.push(name.to_string());
        }
    }

    /// Moves `name` to the front of the recents, dropping the oldest past the cap.
    pub fn touch(&mut self, name: &str) {
        self.recents.retain(|r| r != name);
        self.recents.insert(0, name.to_string());
        self.recents.truncate(MAX_RECENTS);
    }
}

/// `LauncherPrefs` kept in `launcher.json` under the app's data directory.
pub struct PrefsStore {
    path: PathBuf,
    prefs: Mutex<LauncherPrefs>,
}

impl PrefsStore {
    /// A missing or unreadable file starts empty: pins are a convenience, not
    /// something worth refusing to launch over.
    pub fn open(dir: &Path) -> PrefsStore {
        let path = dir.join("launcher.json");
        let prefs = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        PrefsStore {
            path,
            prefs: Mutex::new(prefs),
        }
    }

    pub fn get(&self) -> LauncherPrefs {
        self.prefs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Applies `change`, saves, and returns the result.
    pub fn update(&self, change: impl FnOnce(&mut LauncherPrefs)) -> Result<LauncherPrefs> {
        let mut prefs = self.prefs.lock().unwrap_or_else(PoisonError::into_inner);
        change(&mut prefs);
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).context("creating the data folder")?;
        }
        // Write-then-rename: a crash mid-write must not leave half a file
        // that reads back as "no pins".
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string(&*prefs)?).context("saving launcher.json")?;
        std::fs::rename(&tmp, &self.path).context("saving launcher.json")?;
        Ok(prefs.clone())
    }
}

/// Opens an installed app by name — launching it, or raising it if running.
pub fn open_app(name: &str) -> Result<()> {
    let out = Command::new("open")
        .arg("-a")
        .arg(name)
        .output()
        .context("running open")?;
    anyhow::ensure!(
        out.status.success(),
        "could not open {name}: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinning_twice_keeps_one_tile_and_unpinning_removes_it() {
        let mut p = LauncherPrefs::default();
        p.set_pin("Safari", true);
        p.set_pin("Mail", true);
        p.set_pin("Safari", true);
        assert_eq!(
            p.pins,
            ["Mail", "Safari"],
            "a re-pin moves to the end, never duplicates"
        );
        p.set_pin("Mail", false);
        p.set_pin("Notes", false);
        assert_eq!(p.pins, ["Safari"]);
    }

    #[test]
    fn recents_put_the_latest_first_without_repeats_and_cap_out() {
        let mut p = LauncherPrefs::default();
        for i in 0..20 {
            p.touch(&format!("App {i}"));
        }
        p.touch("App 15");
        assert_eq!(p.recents.len(), MAX_RECENTS);
        assert_eq!(p.recents[0], "App 15");
        assert_eq!(p.recents.iter().filter(|r| *r == "App 15").count(), 1);
        assert!(
            !p.recents.contains(&"App 0".to_string()),
            "the oldest fell off"
        );
    }

    #[test]
    fn prefs_survive_a_restart_and_a_corrupt_file_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = PrefsStore::open(dir.path());
        store.update(|p| p.set_pin("Safari", true)).unwrap();
        assert_eq!(PrefsStore::open(dir.path()).get().pins, ["Safari"]);

        std::fs::write(dir.path().join("launcher.json"), "{not json").unwrap();
        assert_eq!(PrefsStore::open(dir.path()).get(), LauncherPrefs::default());
    }
}
