//! One-shot import of the Swift app's data, macOS only.
//!
//! Two safety rules: only migrate into an untouched destination, and **copy,
//! never move** — the old app is still installed and must keep working.

use crate::Settings;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::{fs, path::PathBuf};

/// Old data homes, newest naming first.
const LEGACY_DIRS: &[&str] = &["Aloud", "Aloud Dev", "FreeFlow"];

/// The Swift `.settings` JSON. Only keys and URLs carry over; models and
/// hotkeys changed shape.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacySettings {
    groq_api_key: String,
    composio_api_key: String,
    api_base_url: String,
    /// Present when the user pointed transcription at a different provider.
    transcription_api_url: String,
    transcription_api_key: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub notes_copied: usize,
    pub settings_imported: bool,
    pub source: Option<PathBuf>,
}

/// Imports legacy data if the new app has none yet. An empty report means there
/// was nothing to do, the case on every launch after the first.
pub fn run() -> Result<Report> {
    if !cfg!(target_os = "macos") {
        return Ok(Report::default()); // the Swift app was macOS-only
    }
    let Some(source) = find_legacy_dir() else {
        return Ok(Report::default());
    };

    tracing::info!(source = %source.display(), "importing data from the Swift app");
    let mut report = Report {
        source: Some(source.clone()),
        ..Default::default()
    };

    report.notes_copied = copy_notes(&source)?;
    report.settings_imported = import_settings(&source)?;

    tracing::info!(
        notes = report.notes_copied,
        settings = report.settings_imported,
        "import finished"
    );
    Ok(report)
}

/// The first legacy directory that exists and has notes worth importing.
fn find_legacy_dir() -> Option<PathBuf> {
    // The Swift app's home sits beside ours, so derive the base rather than
    // recomputing it.
    let new_dir = aloud_notes::data_dir().ok()?;
    let base = new_dir.parent()?.to_path_buf();

    LEGACY_DIRS
        .iter()
        .map(|name| base.join(name))
        .filter(|dir| *dir != new_dir) // never import from ourselves
        .find(|dir| dir.join("Notes").is_dir() || dir.join(".settings").is_file())
}

fn copy_notes(source: &std::path::Path) -> Result<usize> {
    copy_notes_into(&source.join("Notes"), &aloud_notes::directory()?)
}

/// Copies markdown notes across; the format is unchanged. Skips names that
/// already exist — a same-named note in the new home is the user's.
fn copy_notes_into(from: &std::path::Path, to: &std::path::Path) -> Result<usize> {
    if !from.is_dir() {
        return Ok(0);
    }
    fs::create_dir_all(to).with_context(|| format!("creating {}", to.display()))?;

    let mut copied = 0;
    for entry in fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let path = entry?.path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        let target = to.join(name);
        if target.exists() {
            continue;
        }
        fs::copy(&path, &target).with_context(|| format!("copying {}", path.display()))?;
        copied += 1;
    }
    Ok(copied)
}

/// Carries over API keys and base URLs. Returns false when the new app already
/// has settings — the user's current config always wins.
fn import_settings(source: &std::path::Path) -> Result<bool> {
    let legacy_path = source.join(".settings");
    if !legacy_path.is_file() || Settings::path()?.exists() {
        return Ok(false);
    }

    let text = fs::read_to_string(&legacy_path)
        .with_context(|| format!("reading {}", legacy_path.display()))?;
    // A corrupt legacy file must not block first launch.
    let Ok(legacy) = serde_json::from_str::<LegacySettings>(&text) else {
        tracing::warn!(path = %legacy_path.display(), "legacy settings unreadable; skipping");
        return Ok(false);
    };

    let mut settings = Settings {
        groq_api_key: legacy.groq_api_key,
        composio_api_key: legacy.composio_api_key,
        // A Swift user is already set up; don't show them the first-run wizard.
        onboarded: true,
        ..Default::default()
    };

    // Two legacy endpoints collapse into one. Prefer the transcription URL,
    // which is the one users actually repointed.
    if let Some(url) = [legacy.transcription_api_url, legacy.api_base_url]
        .into_iter()
        .find(|u| !u.trim().is_empty())
    {
        settings.base_url = url;
    }
    // A dedicated transcription key means they were not using Groq for it.
    if settings.groq_api_key.trim().is_empty() && !legacy.transcription_api_key.trim().is_empty() {
        settings.groq_api_key = legacy.transcription_api_key;
    }

    // A key means cloud transcription worked; switching them to a model they
    // have not downloaded would break it.
    if !settings.groq_api_key.trim().is_empty() {
        settings.engine = aloud_transcribe::Engine::Cloud;
    }

    settings.save()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_real_legacy_settings_shape() {
        // Exactly the keys the Swift app writes.
        let json = r#"{
            "api_base_url": "https://api.groq.com/openai/v1",
            "composio_api_key": "comp_x",
            "groq_api_key": "gsk_x",
            "keychain_migration_done": true,
            "transcription_api_key": "",
            "transcription_api_url": ""
        }"#;
        let legacy: LegacySettings = serde_json::from_str(json).unwrap();
        assert_eq!(legacy.groq_api_key, "gsk_x");
        assert_eq!(legacy.composio_api_key, "comp_x");
    }

    #[test]
    fn missing_fields_are_tolerated() {
        let legacy: LegacySettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.groq_api_key, "");
    }

    #[test]
    fn copies_only_new_markdown_files() {
        let source = tempfile::tempdir().unwrap();
        let notes = source.path().join("Notes");
        fs::create_dir_all(&notes).unwrap();
        fs::write(notes.join("2026-07-16 1532 Meeting.md"), "# Meeting").unwrap();
        fs::write(notes.join("audio.wav"), "not a note").unwrap();

        let to = tempfile::tempdir().unwrap();
        // Pre-existing note with the same name: must not be clobbered.
        fs::write(to.path().join("2026-07-16 1532 Meeting.md"), "mine").unwrap();

        let copied = copy_notes_into(&notes, to.path()).unwrap();
        assert_eq!(copied, 0, "an existing note is the user's");
        assert_eq!(
            fs::read_to_string(to.path().join("2026-07-16 1532 Meeting.md")).unwrap(),
            "mine",
            "a same-named note must never be overwritten"
        );

        fs::write(notes.join("2026-07-17 0900 Standup.md"), "# Standup").unwrap();
        assert_eq!(copy_notes_into(&notes, to.path()).unwrap(), 1);
        assert!(to.path().join("2026-07-17 0900 Standup.md").exists());
        assert!(!to.path().join("audio.wav").exists(), "only markdown");
    }

    #[test]
    fn missing_notes_dir_is_not_an_error() {
        let to = tempfile::tempdir().unwrap();
        let missing = std::path::Path::new("/nope/Notes");
        assert_eq!(copy_notes_into(missing, to.path()).unwrap(), 0);
    }

    /// Imported notes must be readable by the crate that serves them.
    #[test]
    fn imported_notes_are_readable_by_the_notes_crate() {
        let source = tempfile::tempdir().unwrap();
        let notes = source.path().join("Notes");
        fs::create_dir_all(&notes).unwrap();
        fs::write(
            notes.join("2026-07-16 1532 Quarterly Budget Sync.md"),
            "# Quarterly Budget Sync\n\nDiscussed the hiring plan.",
        )
        .unwrap();

        let to = tempfile::tempdir().unwrap();
        copy_notes_into(&notes, to.path()).unwrap();

        let loaded = aloud_notes::all_notes(to.path());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].title, "Quarterly Budget Sync");
        assert_eq!(aloud_notes::search("hiring", &loaded).len(), 1);
    }
}
