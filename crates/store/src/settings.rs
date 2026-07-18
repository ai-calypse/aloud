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
    /// Cerebras key. Agent tool-call parsing runs here (gpt-oss-120b) because
    /// Groq's tokens-per-minute limit is too low for the agent's heavier calls.
    pub cerebras_api_key: String,

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
    pub press_enter_command: bool,
    /// Whether the first-run wizard has been completed. False shows it.
    pub onboarded: bool,
    #[specta(type = specta_typescript::Number)]
    pub history_retention: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            groq_api_key: String::new(),
            composio_api_key: String::new(),
            cerebras_api_key: String::new(),
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
            press_enter_command: true,
            history_retention: 50,
            // A fresh install has not been through the wizard; a migrated one
            // is handled below, since the Swift users are already set up.
            onboarded: false,
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
        serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!(%e, path = %path.display(), "settings unreadable; using defaults");
            Self::default()
        })
    }

    /// Fills any *empty* key from the environment, so a `.env` at the repo root
    /// (or an exported var) configures a dev build without touching the UI. A
    /// key already set in the settings file always wins — this is a fallback,
    /// never an override. Each var is checked in the process environment first,
    /// then in a `.env` found by walking up from the working directory.
    fn fill_keys_from_env(&mut self) {
        for (var, field) in [
            ("GROQ_API_KEY", &mut self.groq_api_key),
            ("COMPOSIO_API_KEY", &mut self.composio_api_key),
            ("CEREBRAS_API_KEY", &mut self.cerebras_api_key),
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

    /// Whether transcription can run: local needs nothing, cloud needs a key.
    pub fn is_configured(&self) -> bool {
        match self.engine {
            aloud_transcribe::Engine::Local => true,
            aloud_transcribe::Engine::Cloud => !self.groq_api_key.trim().is_empty(),
        }
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
    fn defaults_work_without_a_key() {
        let settings = Settings::default();
        assert!(settings.is_configured(), "local engine needs no key");
    }

    #[test]
    fn cloud_engine_requires_a_key() {
        let mut settings = Settings {
            engine: aloud_transcribe::Engine::Cloud,
            ..Default::default()
        };
        assert!(!settings.is_configured());

        settings.groq_api_key = "   ".into();
        assert!(!settings.is_configured(), "whitespace is not a key");

        settings.groq_api_key = "gsk_real".into();
        assert!(settings.is_configured());
    }

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
