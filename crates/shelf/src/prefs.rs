//! Which watchers are on. Its own small file rather than a field on
//! `aloud_store::Settings`, so the Shelf lands without touching the settings
//! schema (and its migrations) that every other feature shares.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// What the Shelf watches. Screenshots and Downloads read folders macOS
/// guards behind a permission prompt, so they start off: the first prompt
/// should come from the user turning the feature on, not from launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
#[serde(rename = "ShelfPrefs")]
pub struct Prefs {
    pub shots: bool,
    pub downloads: bool,
    /// `/Volumes` is not protected, so this one needs no permission.
    pub drives: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            shots: false,
            downloads: false,
            drives: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShelfWatch {
    Shots,
    Downloads,
    Drives,
}

impl Prefs {
    /// A missing or unreadable file is the defaults: never block startup on it.
    pub fn load(file: &Path) -> Self {
        std::fs::read(file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> anyhow::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(file, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn set(&mut self, which: ShelfWatch, on: bool) {
        match which {
            ShelfWatch::Shots => self.shots = on,
            ShelfWatch::Downloads => self.downloads = on,
            ShelfWatch::Drives => self.drives = on,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_from_an_older_build_keeps_new_fields_at_their_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("prefs.json");
        std::fs::write(&file, r#"{"shots": true}"#).unwrap();
        let prefs = Prefs::load(&file);
        assert!(prefs.shots);
        assert!(!prefs.downloads);
        assert!(prefs.drives);
    }

    #[test]
    fn garbage_is_the_defaults_not_a_failed_launch() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("prefs.json");
        std::fs::write(&file, "not json").unwrap();
        assert_eq!(Prefs::load(&file), Prefs::default());
    }

    #[test]
    fn set_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sub/prefs.json");
        let mut prefs = Prefs::default();
        prefs.set(ShelfWatch::Downloads, true);
        prefs.save(&file).unwrap();
        assert!(Prefs::load(&file).downloads);
    }
}
