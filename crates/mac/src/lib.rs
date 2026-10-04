//! The notch's Mac section, from public macOS APIs only: system stats,
//! battery, which apps hold the mic or camera, and window snapping. No Tauri
//! here — `src-tauri/src/mac.rs` wires these to commands and islands, so
//! everything that can be quietly wrong (parsing, thresholds, geometry) is
//! under test in this crate.
//!
//! Keyboard backlight, external-display brightness and menu-bar icon hiding
//! are not here on purpose: each needs private frameworks (CoreBrightness,
//! IOAVService, the window server's menu-extra layer), which a notarized,
//! non-sandboxed app can call but Apple can remove in any release.

use std::path::{Path, PathBuf};

use aloud_islands::Island;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub mod power;
pub mod privacy;
pub mod stats;
pub mod windows;

/// What a watcher decided, for the app to apply to the island queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Push(Island),
    Remove(&'static str),
}

/// The two background watchers, each with an on/off the user owns. Both
/// default on: they are quiet until there is something worth a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Watchers {
    /// An island while any app is using the microphone or camera.
    pub privacy: bool,
    /// An island on a full charge, and while the battery is low.
    pub battery: bool,
}

impl Default for Watchers {
    fn default() -> Self {
        Self {
            privacy: true,
            battery: true,
        }
    }
}

/// Which watcher a switch names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Watcher {
    Privacy,
    Battery,
}

impl Watchers {
    pub fn set(&mut self, which: Watcher, on: bool) {
        match which {
            Watcher::Privacy => self.privacy = on,
            Watcher::Battery => self.battery = on,
        }
    }

    /// A lone file beside the settings, not a field in them: the switches are
    /// read by a thread at launch and written by one command, and a settings
    /// field would make every notch feature edit `aloud_store`.
    pub fn path() -> Result<PathBuf> {
        Ok(aloud_notes::data_dir()?.join("mac-watchers.json"))
    }

    /// A missing or corrupt file is the defaults: a watcher must never be off
    /// because a write was cut short.
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        std::fs::write(path, serde_json::to_string(self)?)
            .with_context(|| format!("writing {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_corrupt_or_missing_file_leaves_both_watchers_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.json");
        assert_eq!(Watchers::load_from(&path), Watchers::default());
        std::fs::write(&path, "{ nope").unwrap();
        assert_eq!(Watchers::load_from(&path), Watchers::default());
    }

    #[test]
    fn a_switch_survives_a_restart_and_an_older_file_keeps_new_ones_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.json");
        let mut w = Watchers::default();
        w.set(Watcher::Privacy, false);
        w.save_to(&path).unwrap();
        assert!(!Watchers::load_from(&path).privacy);
        assert!(Watchers::load_from(&path).battery);

        // A file written before `battery` existed must not turn it off.
        std::fs::write(&path, r#"{"privacy":false}"#).unwrap();
        assert_eq!(
            Watchers::load_from(&path),
            Watchers {
                privacy: false,
                battery: true
            }
        );
    }
}
