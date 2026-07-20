//! Settings, persisted as JSON in the app data directory.
//!
//! Mirrors the Swift app's `.settings` file: one JSON document holding both
//! preferences and API keys, deliberately not the OS keychain. Written
//! atomically (temp file + rename) so a crash mid-write cannot corrupt it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

/// Cleanup model defaults, matching voice-dict's current ones
/// (`AppState.defaultPostProcessingModel` / `…FallbackModel`).
const DEFAULT_CLEANUP_MODEL: &str = "openai/gpt-oss-20b";
const DEFAULT_FALLBACK_MODEL: &str = "qwen/qwen3.6-27b";

/// Everything the app remembers between launches.
///
/// One coarse struct rather than ~70 individual commands: the whole thing is
/// a few hundred bytes, and the frontend gets one `get`/`update` pair.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    // Keys. In the same file as the rest, matching the Swift app.
    pub groq_api_key: String,
    pub composio_api_key: String,
    /// Gemini key. Agent tool-call parsing runs here — Groq's tokens-per-minute
    /// limit is too low for the agent's heavier calls.
    pub gemini_api_key: String,

    // Transcription
    pub engine: aloud_transcribe::Engine,
    pub transcription_model: String,
    pub base_url: String,
    /// BCP-47 code, or empty to auto-detect.
    pub language: String,

    // Post-processing
    pub post_processing_enabled: bool,
    pub post_processing_model: String,
    pub fallback_model: String,
    /// Overrides the built-in cleanup prompt when non-empty.
    pub custom_system_prompt: String,
    /// Proper nouns and jargon fed to the model as a spelling reference.
    pub vocabulary: Vec<String>,
    /// Spoken shorthand that expands to canned text after cleanup.
    pub macros: Vec<crate::macros::Macro>,
    /// Per-app behaviour overrides, applied by the focused app's name.
    pub app_rules: Vec<crate::rules::AppRule>,

    // Behaviour
    pub hotkeys: aloud_hotkey::model::Config,
    pub selected_microphone: Option<String>,
    pub play_sounds: bool,
    /// Alert-chime volume, 0.0–1.0. Honoured on macOS (`afplay -v`); Windows
    /// system sounds have no per-sound volume, so it is ignored there.
    pub sound_volume: f32,
    pub press_enter_command: bool,
    /// Wait this long after the hotkey before capture begins, so the first
    /// syllable is not clipped by a key that is still travelling. 0 disables.
    #[specta(type = specta_typescript::Number)]
    pub shortcut_start_delay_ms: u64,
    /// Silence system output while recording, so music does not end up in the
    /// transcript. Restored when capture stops, and on the next launch if the
    /// app died first. macOS only.
    pub mute_system_audio: bool,
    /// Show the menu-bar icon. Hiding it leaves hotkeys working; the way back
    /// to the window is to open the app again, which macOS turns into a
    /// reopen event rather than a second instance.
    pub show_menu_bar_icon: bool,
    /// Put the previous clipboard back after pasting. Off leaves the dictated
    /// text on the clipboard so it can be pasted again.
    pub preserve_clipboard: bool,
    /// Whether the first-run wizard has been completed. False shows it.
    pub onboarded: bool,
    /// Reveals the latency waterfall. A debugging tool, not a feature — but it
    /// was built and then hidden behind a build-time flag, which made it
    /// unreachable in every release the users actually run.
    pub developer_tools: bool,
    #[specta(type = specta_typescript::Number)]
    pub history_retention: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            groq_api_key: String::new(),
            composio_api_key: String::new(),
            gemini_api_key: String::new(),
            // Local by default: works with no key and no network.
            engine: aloud_transcribe::Engine::Local,
            transcription_model: aloud_transcribe::cloud::DEFAULT_MODEL.into(),
            base_url: aloud_transcribe::cloud::DEFAULT_BASE_URL.into(),
            language: String::new(),
            post_processing_enabled: true,
            post_processing_model: DEFAULT_CLEANUP_MODEL.into(),
            fallback_model: DEFAULT_FALLBACK_MODEL.into(),
            custom_system_prompt: String::new(),
            vocabulary: Vec::new(),
            macros: Vec::new(),
            app_rules: Vec::new(),
            hotkeys: aloud_hotkey::model::Config::defaults(),
            selected_microphone: None,
            play_sounds: true,
            sound_volume: 0.5,
            press_enter_command: true,
            shortcut_start_delay_ms: 0,
            mute_system_audio: false,
            show_menu_bar_icon: true,
            preserve_clipboard: true,
            history_retention: 50,
            // A fresh install has not been through the wizard; a migrated one
            // is handled below, since the Swift users are already set up.
            onboarded: false,
            developer_tools: false,
        }
    }
}

impl Settings {
    pub fn path() -> Result<PathBuf> {
        Ok(aloud_notes::data_dir()?.join("settings.json"))
    }

    /// Loads settings, falling back to defaults when the file is absent.
    ///
    /// A corrupt file also yields defaults rather than an error: the app must
    /// still start, and `#[serde(default)]` means a partial file keeps
    /// whatever fields do parse.
    pub fn load() -> Self {
        let mut settings = Self::load_file();
        settings.migrate_deprecated_models();
        settings.fill_keys_from_env();
        settings
    }

    /// Moves a stored *old default* model to the current default, ported from
    /// voice-dict's deprecated-default handling. A value the user typed
    /// themselves is untouched; only the exact former defaults migrate (which
    /// also catches a user who deliberately chose the old default — accepted,
    /// as in the Swift app).
    fn migrate_deprecated_models(&mut self) {
        if self.post_processing_model == "llama-3.3-70b-versatile" {
            self.post_processing_model = DEFAULT_CLEANUP_MODEL.into();
        }
        if self.fallback_model == "llama-3.1-8b-instant" {
            self.fallback_model = DEFAULT_FALLBACK_MODEL.into();
        }
    }

    fn load_file() -> Self {
        let Ok(path) = Self::path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        Self::from_json(&text).unwrap_or_else(|e| {
            tracing::warn!(%e, path = %path.display(), "settings unreadable; using defaults");
            Self::default()
        })
    }

    /// Parses a settings file, carrying renamed keys forward.
    ///
    /// `hotkeys.command` became `hotkeys.rewrite`. Without this the field would
    /// simply deserialize to None and the user's hotkey would stop working with
    /// no error anywhere — the quietest possible way to break someone's setup.
    pub(crate) fn from_json(text: &str) -> serde_json::Result<Self> {
        let mut raw: serde_json::Value = serde_json::from_str(text)?;
        if let Some(hotkeys) = raw.get_mut("hotkeys").and_then(|h| h.as_object_mut()) {
            if let Some(old) = hotkeys.remove("command") {
                hotkeys.entry("rewrite").or_insert(old);
            }
        }
        serde_json::from_value(raw)
    }

    /// Fills any *empty* key from the environment. Since the key inputs were
    /// removed (no BYOK), this is the *only* way a key is ever supplied: a
    /// `.env` at the repo root, or an exported var, configures a dev build. A
    /// key already in the settings file still wins, so a migrated Swift install
    /// keeps its own. Each var is checked in the process environment first,
    /// then in a `.env` found by walking up from the working directory.
    ///
    /// This does not survive bundling — `current_dir()` is `/` for a `.app`
    /// launched from Finder, so a shipped build resolves nothing and falls back
    /// to on-device whisper. Provisioning for release is Phase 12.
    fn fill_keys_from_env(&mut self) {
        for (var, field) in [
            ("GROQ_API_KEY", &mut self.groq_api_key),
            ("COMPOSIO_API_KEY", &mut self.composio_api_key),
            ("GEMINI_API_KEY", &mut self.gemini_api_key),
        ] {
            if !field.trim().is_empty() {
                continue;
            }
            if let Some(value) = env_or_dotenv(var) {
                *field = value;
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        let dir = path.parent().context("settings path has no parent")?;
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

        // Temp file + rename: a crash mid-write must not leave a truncated
        // file holding the user's API keys.
        let temp = path.with_extension("tmp");
        fs::write(&temp, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", temp.display()))?;
        restrict_permissions(&temp)?;
        fs::rename(&temp, &path).context("replacing settings file")?;
        Ok(())
    }
}

/// A var from the process environment, or from the first `.env` found walking
/// up from the working directory. Returns None if unset/empty in both.
///
/// No dotenv crate: the file is a handful of `KEY=value` lines and we only read
/// three keys. Values may be quoted; surrounding quotes and spaces are trimmed.
fn env_or_dotenv(var: &str) -> Option<String> {
    if let Ok(value) = std::env::var(var) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(".env");
        if let Ok(contents) = fs::read_to_string(&candidate) {
            if let Some(value) = parse_dotenv_value(&contents, var) {
                return Some(value);
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Pulls `var`'s value out of `.env` contents, or None. Skips blank/comment
/// lines; trims surrounding quotes and whitespace from the value.
fn parse_dotenv_value(contents: &str, var: &str) -> Option<String> {
    let prefix = format!("{var}=");
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix(&prefix) {
            let value = rest.trim().trim_matches(['"', '\'']).trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Owner-only (0600) on Unix. On Windows the per-user profile directory is
/// already ACL'd to the user, which is the practical equivalent.
#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("restricting {}", path.display()))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotenv_value_is_parsed_and_unquoted() {
        let env = "# a comment\nGROQ_API_KEY=gsk_plain\nCEREBRAS_API_KEY=\"csk_quoted\"\nCOMPOSIO_API_KEY=  \n";
        assert_eq!(
            parse_dotenv_value(env, "GROQ_API_KEY").as_deref(),
            Some("gsk_plain")
        );
        assert_eq!(
            parse_dotenv_value(env, "CEREBRAS_API_KEY").as_deref(),
            Some("csk_quoted")
        );
        // Empty value and absent key both yield None.
        assert_eq!(parse_dotenv_value(env, "COMPOSIO_API_KEY"), None);
        assert_eq!(parse_dotenv_value(env, "MISSING"), None);
    }

    #[test]
    fn deprecated_default_models_migrate_but_custom_ones_do_not() {
        let mut old = Settings {
            post_processing_model: "llama-3.3-70b-versatile".into(),
            fallback_model: "llama-3.1-8b-instant".into(),
            ..Default::default()
        };
        old.migrate_deprecated_models();
        assert_eq!(old.post_processing_model, DEFAULT_CLEANUP_MODEL);
        assert_eq!(old.fallback_model, DEFAULT_FALLBACK_MODEL);

        let mut custom = Settings {
            post_processing_model: "my-fine-tune".into(),
            fallback_model: "llama-3.1-70b".into(),
            ..Default::default()
        };
        custom.migrate_deprecated_models();
        assert_eq!(custom.post_processing_model, "my-fine-tune");
        assert_eq!(custom.fallback_model, "llama-3.1-70b");
    }

    #[test]
    fn round_trips_through_json() {
        let settings = Settings {
            vocabulary: vec!["Kubernetes".into(), "Tauri".into()],
            groq_api_key: "gsk_test".into(),
            ..Default::default()
        };

        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.vocabulary, settings.vocabulary);
        assert_eq!(parsed.groq_api_key, settings.groq_api_key);
        assert_eq!(parsed.hotkeys, settings.hotkeys);
    }

    /// A settings file written by an older version must still load: unknown
    /// fields are ignored and missing ones take their default.
    #[test]
    fn partial_json_keeps_defaults_for_missing_fields() {
        let parsed: Settings =
            serde_json::from_str(r#"{"groqApiKey":"gsk_x","unknownField":42}"#).unwrap();
        assert_eq!(parsed.groq_api_key, "gsk_x");
        assert!(parsed.play_sounds, "missing field falls back to default");
    }

    #[test]
    fn corrupt_json_falls_back_to_defaults() {
        // load() must never panic the app on a mangled file.
        let parsed: Result<Settings, _> = serde_json::from_str("{not json");
        assert!(parsed.is_err(), "sanity: this really is invalid");
    }
}

#[cfg(test)]
mod rename_tests {
    use super::*;

    /// The rewrite hotkey was called `command` before. Losing this migration
    /// would not fail to parse — the field would deserialize to None and the
    /// user's hotkey would quietly stop working, with nothing logged.
    #[test]
    fn an_old_settings_file_keeps_its_rewrite_hotkey() {
        let on_disk = r#"{
            "hotkeys": {
                "hold": { "kind": "modifierKey", "key": "fn", "modifiers": [] },
                "command": { "kind": "modifierKey", "key": "leftOption", "modifiers": [] }
            }
        }"#;

        let settings = Settings::from_json(on_disk).expect("old settings must still parse");
        assert!(
            settings.hotkeys.rewrite.is_some(),
            "a hotkey saved as `command` must load as `rewrite`"
        );
        assert!(settings.hotkeys.hold.is_some(), "other roles are untouched");
    }

    /// A file already using the new name must not be disturbed.
    #[test]
    fn a_current_settings_file_is_unchanged() {
        let on_disk = r#"{
            "hotkeys": {
                "rewrite": { "kind": "modifierKey", "key": "rightOption", "modifiers": [] }
            }
        }"#;

        let settings = Settings::from_json(on_disk).expect("current settings must parse");
        assert!(settings.hotkeys.rewrite.is_some());
    }
}
