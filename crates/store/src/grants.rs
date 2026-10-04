//! Saved permission grants, one per app and action ("Slack: send and share"),
//! in `grants.json` in the app data directory.
//!
//! Its own file, like `stats.json`, not a corner of settings.json:
//! `update_settings` writes back the frontend's whole copy of the settings and
//! listen.rs snapshots them at session start, so a grant stored there could be
//! clobbered by a stale copy or read late.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

/// What the user chose for one app and action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Grant {
    Always,
    Ask,
    Never,
}

/// One row of `grants.json`, carrying the labels the Settings pane shows so it
/// never has to unslug a key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedGrant {
    pub app: String,
    pub action: String,
    pub grant: Grant,
}

/// Every saved grant by key: `"{app}:{action}"` slugged, e.g. `slack:send-and-share`.
pub type Grants = BTreeMap<String, SavedGrant>;

/// What the gate does with a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Run,
    /// Show the card. `can_always` is false for a strict action, which an
    /// "Always allow" must never cover.
    Prompt {
        can_always: bool,
    },
    Refuse,
}

/// Never refuses without a card; strict always asks, whatever was saved; then
/// a saved Always, or no saved grant on a tool that defaults to Always, runs.
pub fn decide(stored: Option<Grant>, default_always: bool, strict: bool) -> Decision {
    match stored {
        Some(Grant::Never) => Decision::Refuse,
        _ if strict => Decision::Prompt { can_always: false },
        Some(Grant::Always) => Decision::Run,
        None if default_always => Decision::Run,
        _ => Decision::Prompt { can_always: true },
    }
}

/// `<app-data>/Aloud/grants.json`.
pub fn path() -> Result<PathBuf> {
    Ok(aloud_notes::data_dir()?.join("grants.json"))
}

/// The saved grant for `key`. Read from disk every call: the file is a few
/// rows, and a cache would be one more thing the Settings pane must invalidate.
pub fn get(key: &str) -> Option<Grant> {
    all().get(key).map(|saved| saved.grant)
}

/// Every saved grant; empty when absent or unreadable.
pub fn all() -> Grants {
    path().map(|p| load(&p)).unwrap_or_default()
}

pub fn set(key: &str, app: &str, action: &str, grant: Grant) -> Result<()> {
    set_in(&path()?, key, app, action, grant)
}

// Path-injectable halves, so tests never touch the real grants.json.

/// A missing or corrupt file reads as no grants: every action then falls back
/// to its tool's default, which asks for anything not known to be harmless.
fn load(path: &Path) -> Grants {
    let Ok(text) = fs::read_to_string(path) else {
        return Grants::new();
    };
    serde_json::from_str(&text).unwrap_or_else(|e| {
        tracing::warn!(%e, "grants unreadable; every action falls back to its default");
        Grants::new()
    })
}

/// Read-modify-write behind one lock, so a click in Settings and an "Always
/// allow" on a card cannot each write back a copy missing the other's row.
/// Temp + rename, so a crash mid-write cannot leave half a file.
fn set_in(path: &Path, key: &str, app: &str, action: &str, grant: Grant) -> Result<()> {
    static WRITE: Mutex<()> = Mutex::new(());
    let _held = WRITE.lock().unwrap_or_else(PoisonError::into_inner);
    let mut grants = load(path);
    grants.insert(
        key.to_string(),
        SavedGrant {
            app: app.to_string(),
            action: action.to_string(),
            grant,
        },
    );
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("tmp");
    fs::write(&temp, serde_json::to_string_pretty(&grants)?)?;
    fs::rename(&temp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_allow_cannot_skip_a_strict_action() {
        assert_eq!(
            decide(Some(Grant::Always), true, true),
            Decision::Prompt { can_always: false }
        );
        assert_eq!(
            decide(None, false, true),
            Decision::Prompt { can_always: false }
        );
    }

    #[test]
    fn never_refuses_without_asking() {
        assert_eq!(decide(Some(Grant::Never), true, false), Decision::Refuse);
        assert_eq!(decide(Some(Grant::Never), false, true), Decision::Refuse);
    }

    #[test]
    fn no_saved_grant_falls_back_to_the_tools_default() {
        assert_eq!(decide(None, true, false), Decision::Run);
        assert_eq!(
            decide(None, false, false),
            Decision::Prompt { can_always: true }
        );
        // A saved Ask beats a default of Always: the user made notes ask.
        assert_eq!(
            decide(Some(Grant::Ask), true, false),
            Decision::Prompt { can_always: true }
        );
        assert_eq!(decide(Some(Grant::Always), false, false), Decision::Run);
    }

    #[test]
    fn grants_survive_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Aloud").join("grants.json");
        assert!(load(&path).is_empty());

        set_in(
            &path,
            "slack:send-and-share",
            "slack",
            "send and share",
            Grant::Always,
        )
        .unwrap();
        set_in(
            &path,
            "notes:create-and-edit",
            "Notes",
            "create and edit",
            Grant::Ask,
        )
        .unwrap();
        set_in(
            &path,
            "slack:send-and-share",
            "slack",
            "send and share",
            Grant::Never,
        )
        .unwrap();

        let reloaded = load(&path);
        assert_eq!(reloaded.len(), 2, "a second set replaces, not appends");
        assert_eq!(reloaded["slack:send-and-share"].grant, Grant::Never);
        assert_eq!(
            reloaded["notes:create-and-edit"],
            SavedGrant {
                app: "Notes".into(),
                action: "create and edit".into(),
                grant: Grant::Ask,
            }
        );
        assert!(!path.with_extension("tmp").exists());

        fs::write(&path, "{not json").unwrap();
        assert!(load(&path).is_empty(), "a corrupt file reads as no grants");
    }
}
