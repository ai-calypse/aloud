//! Settings, persisted as JSON in the app data directory.
//!
//! One document holding both preferences and API keys, deliberately not the OS
//! keychain. Written atomically (temp + rename) so a crash cannot corrupt it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

const DEFAULT_CLEANUP_MODEL: &str = "openai/gpt-oss-20b";
const DEFAULT_FALLBACK_MODEL: &str = "qwen/qwen3.8-27b";

/// Longest vocabulary term accepted from the clipboard. Every term is re-sent
/// with every cleanup request, so a stray copied paragraph is paid for forever.
const MAX_TERM_CHARS: usize = 48;

/// How the recording overlay draws itself. `DropPill` grows out of the notch
/// and has room for content; `Wings` are two menu-bar tabs (macOS only, and
/// they fall back to the pill with no notch or text too long for a 40pt tab).
///
/// `DropPill` serializes as `dropPill`, never the `pill` its name suggests:
/// `pill` was the retired glass capsule's stored name, and the collision made
/// this style revert on every restart — see
/// `the_drop_down_pill_survives_a_restart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OverlayStyle {
    DropPill,
    Wings,
}

/// How the open notch panel looks. `Calm` is the monochrome panel; `Widgets`
/// redraws it as Apple-style widgets (docs/design/notch-personality.html). Only
/// the look changes: the sections, shortcuts and closed notch are the same.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum NotchStyle {
    #[default]
    Calm,
    Widgets,
}

/// Which monitor the overlay appears on. `Named` stores the OS name, not an
/// index: indices are reassigned on unplug, so a saved index changes meaning.
/// A name that no longer resolves falls back to the primary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OverlayDisplay {
    /// The monitor holding the cursor.
    Active,
    Primary,
    Named(String),
}

/// Removes `key` from a settings document when it no longer parses as `T`.
fn drop_if_unparseable<T: serde::de::DeserializeOwned>(raw: &mut serde_json::Value, key: &str) {
    if raw
        .get(key)
        .is_some_and(|value| serde_json::from_value::<T>(value.clone()).is_err())
    {
        if let Some(object) = raw.as_object_mut() {
            object.remove(key);
        }
    }
}

/// Everything the app remembers between launches. One coarse struct rather
/// than ~70 commands: it is a few hundred bytes and the frontend gets one
/// `get`/`update` pair.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub groq_api_key: String,
    pub composio_api_key: String,
    /// Gemini key: the brain behind Groq — see `aloud_agent::brains` for the
    /// order.
    pub gemini_api_key: String,
    /// Cerebras key. Off the brain list while its account is unpaid; the eval
    /// still reaches it by name.
    pub cerebras_api_key: String,
    /// NVIDIA key: the last brain, for when Groq's and Gemini's budgets trip.
    pub nvidia_api_key: String,
    /// Deepgram key. Always-listening streams the mic to Deepgram's Flux, which
    /// decides where a spoken request ends; without it the mode falls back to a
    /// pause detector and local whisper.
    pub deepgram_api_key: String,

    // Transcription
    pub engine: aloud_transcribe::Engine,
    pub transcription_model: String,
    pub base_url: String,
    /// BCP-47 code, or empty to auto-detect.
    pub language: String,
    /// Live-transcript preview. Changes nothing about what gets pasted — see
    /// [`aloud_transcribe::preview`]. Off by default: it runs the local model
    /// several times a second, which is real CPU and battery on a laptop.
    pub streaming_transcription: bool,

    // Post-processing
    pub post_processing_enabled: bool,
    pub post_processing_model: String,
    pub fallback_model: String,
    /// Overrides the built-in cleanup prompt when non-empty.
    pub custom_system_prompt: String,
    /// Version of the built-in prompt the user last acknowledged; empty means
    /// never, so a pre-existing install sees the offer once.
    pub seen_prompt_version: String,
    /// Language the pasted text is written in — distinct from [`language`],
    /// the language *spoken*. Cleanup applies it; transcription never sees it.
    ///
    /// [`language`]: Settings::language
    pub output_language: String,
    /// Reject a cleanup that *answered* the transcript instead of tidying it —
    /// see [`aloud_llm::guard`]. On by default: it catches pasting text the
    /// user never dictated, and a false positive only costs the raw transcript.
    pub instruction_guard_enabled: bool,
    /// Proper nouns and jargon fed to the model as a spelling reference, and to
    /// the transcriber as a recognition bias. See `aloud_transcribe::bias_prompt`.
    pub vocabulary: Vec<String>,
    /// Phrases cleanup must return exactly as spoken — no filler removal, no
    /// rewording, no punctuation "fixes" inside them.
    ///
    /// Distinct from [`vocabulary`], which only steers *spelling* and leaves the
    /// model free to tidy the sentence around it. "I am um boss" needs the "um"
    /// kept, and the filler rule would otherwise delete it.
    ///
    /// [`vocabulary`]: Settings::vocabulary
    pub preserve_phrases: Vec<String>,
    /// Spoken shorthand that expands to canned text after cleanup.
    pub macros: Vec<crate::macros::Macro>,
    /// Per-app behaviour overrides, applied by the focused app's name.
    pub app_rules: Vec<crate::rules::AppRule>,

    // Command mode — dictating over a selection rewrites it instead.
    /// Whether a selection can turn a dictation into a rewrite. A selection is
    /// the *only* signal — no extra-modifier gesture, which collided with the
    /// dictate key. Explicit rewrite is [`aloud_hotkey::model::Role::Rewrite`].
    ///
    /// On by default: the feature shipped with no toggle, so defaulting to off
    /// would silently remove it from every existing user.
    pub command_mode_enabled: bool,

    // Behaviour
    pub hotkeys: aloud_hotkey::model::Config,
    pub selected_microphone: Option<String>,
    pub play_sounds: bool,
    /// Alert-chime volume, 0.0–1.0. Honoured on macOS (`afplay -v`); Windows
    /// system sounds have no per-sound volume, so it is ignored there.
    pub sound_volume: f32,
    pub press_enter_command: bool,
    /// Delay before capture begins, so a key still travelling does not clip the
    /// first syllable. 0 disables.
    #[specta(type = specta_typescript::Number)]
    pub shortcut_start_delay_ms: u64,
    /// Silence system output while recording. Restored when capture stops, and
    /// on the next launch if the app died first. macOS only.
    pub mute_system_audio: bool,
    /// Show the menu-bar icon. Hiding it leaves hotkeys working; opening the app
    /// again is the way back to the window (macOS turns that into a reopen).
    pub show_menu_bar_icon: bool,
    /// Drop-down pill or menu-bar wings; wings are macOS-only.
    pub overlay_style: OverlayStyle,
    /// Calm (monochrome) or Widgets (Apple-style tiles) for the open notch.
    pub notch_style: NotchStyle,
    /// Whether a coding agent's permission request opens the notch by itself.
    /// Off means the request waits quietly — the Agents surface flags it, and
    /// the user opens the notch when they hover. The hook waits either way.
    pub agent_open_on_permission: bool,
    /// Whether the notch's Now Playing also reads video and audio playing in a
    /// browser tab. It reads only media state (title, host, position), but it
    /// does script the browser, which asks macOS for Automation permission.
    pub browser_media: bool,
    /// Whether the notch's Now Playing shows whatever any app plays, through
    /// macOS's private MediaRemote interface (`aloud_media::mediaremote`). Off
    /// means the helper process is not running at all.
    pub any_app_media: bool,
    pub overlay_display: OverlayDisplay,
    /// Put the previous clipboard back after pasting. Off leaves the dictated
    /// text on the clipboard so it can be pasted again.
    pub preserve_clipboard: bool,
    /// Whether the first-run wizard has been completed. False shows it.
    pub onboarded: bool,
    /// Reveals the latency waterfall. A runtime toggle, not a build-time flag,
    /// which made it unreachable in every release users actually run.
    pub developer_tools: bool,
    #[specta(type = specta_typescript::Number)]
    pub history_retention: usize,
    /// Disk the kept recordings may use, in MB. 0 keeps none, which also
    /// removes the Run Log's player and retry — both read the WAV.
    #[specta(type = specta_typescript::Number)]
    pub audio_retention_mb: u64,
    /// Evict recordings older than this many days regardless of the size
    /// budget. 0 leaves size as the only limit.
    pub audio_retention_days: u32,
    /// Master switch for the meme cats. Off hides every cat everywhere; the
    /// per-place flags below only matter while this is on. On by default —
    /// the cats are the personality, opting out is the exception.
    pub cat_images: bool,
    /// The "I am listening" cat that rides the notch during a live dictation.
    pub cat_images_dictation: bool,
    /// The stage cats on the agents surface — the ambient strip, the dealt
    /// notch cards, the agent list and its hover peek.
    pub cat_images_agents: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            groq_api_key: String::new(),
            composio_api_key: String::new(),
            gemini_api_key: String::new(),
            cerebras_api_key: String::new(),
            nvidia_api_key: String::new(),
            deepgram_api_key: String::new(),
            // Local by default: works with no key and no network.
            engine: aloud_transcribe::Engine::Local,
            transcription_model: aloud_transcribe::cloud::DEFAULT_MODEL.into(),
            base_url: aloud_transcribe::cloud::DEFAULT_BASE_URL.into(),
            language: String::new(),
            streaming_transcription: false,
            post_processing_enabled: true,
            post_processing_model: DEFAULT_CLEANUP_MODEL.into(),
            fallback_model: DEFAULT_FALLBACK_MODEL.into(),
            custom_system_prompt: String::new(),
            seen_prompt_version: String::new(),
            output_language: String::new(),
            instruction_guard_enabled: true,
            vocabulary: Vec::new(),
            preserve_phrases: Vec::new(),
            macros: Vec::new(),
            app_rules: Vec::new(),
            command_mode_enabled: true,
            hotkeys: aloud_hotkey::model::Config::defaults(),
            selected_microphone: None,
            play_sounds: true,
            sound_volume: 0.5,
            press_enter_command: true,
            shortcut_start_delay_ms: 0,
            mute_system_audio: false,
            show_menu_bar_icon: true,
            overlay_style: OverlayStyle::DropPill,
            notch_style: NotchStyle::Calm,
            // On: a blocked agent is usually why the user looks up. The
            // toggle exists for people who find the self-opening notch rude.
            agent_open_on_permission: true,
            browser_media: true,
            any_app_media: true,
            overlay_display: OverlayDisplay::Active,
            preserve_clipboard: true,
            history_retention: 50,
            // Off deliberately: `#[serde(default)]` means a non-zero value here
            // would start writing raw audio to disk for existing users on
            // upgrade, unprompted. (250 MB ≈ four hours at 16 kHz mono.)
            audio_retention_mb: 0,
            audio_retention_days: 30,
            onboarded: false,
            developer_tools: false,
            // The cats ship on — they are the app's personality, not an add-on.
            cat_images: true,
            cat_images_dictation: true,
            cat_images_agents: true,
        }
    }
}

impl Settings {
    /// Retention as the history store wants it, so the MB-to-byte conversion
    /// lives in one place.
    pub fn retention(&self) -> crate::history::Retention {
        crate::history::Retention {
            entries: self.history_retention,
            audio_bytes: self.audio_retention_mb.saturating_mul(1024 * 1024),
            audio_days: self.audio_retention_days.into(),
        }
    }

    pub fn path() -> Result<PathBuf> {
        Ok(aloud_notes::data_dir()?.join("settings.json"))
    }

    /// Loads settings; an absent or corrupt file yields defaults rather than an
    /// error, because the app must still start.
    pub fn load() -> Self {
        let mut settings = Self::load_file();
        settings.migrate_deprecated_models();
        settings.fill_keys_from_env();
        settings
    }

    /// Moves a stored *old default* model to the current default. Only the
    /// exact former defaults migrate; a value the user typed is untouched.
    fn migrate_deprecated_models(&mut self) {
        if self.post_processing_model == "llama-3.3-70b-versatile" {
            self.post_processing_model = DEFAULT_CLEANUP_MODEL.into();
        }
        // qwen3.6-27b left Groq's model list: a cleanup falling back to it 404s.
        if matches!(
            self.fallback_model.as_str(),
            "llama-3.1-8b-instant" | "qwen/qwen3.6-27b"
        ) {
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
    /// `hotkeys.command` became `hotkeys.rewrite`; without this the field
    /// deserializes to None and the user's hotkey stops working silently.
    pub(crate) fn from_json(text: &str) -> serde_json::Result<Self> {
        let mut raw: serde_json::Value = serde_json::from_str(text)?;
        if let Some(hotkeys) = raw.get_mut("hotkeys").and_then(|h| h.as_object_mut()) {
            if let Some(old) = hotkeys.remove("command") {
                hotkeys.entry("rewrite").or_insert(old);
            }
        }
        // An unrecognised `overlayStyle` (or `notchStyle`, from a newer build)
        // is dropped so the field defaults. Asking serde whether it parses
        // beats a per-dead-value rename list, which once collided with a live
        // name. The document must not fail over one stale enum — that would
        // reset the file, and the API keys are in it.
        drop_if_unparseable::<OverlayStyle>(&mut raw, "overlayStyle");
        drop_if_unparseable::<NotchStyle>(&mut raw, "notchStyle");
        serde_json::from_value(raw)
    }

    /// Fills any *empty* key from the environment or a `.env` walked up to from
    /// the working directory — the only way a key is supplied (no BYOK inputs).
    /// A key already in the settings file wins.
    ///
    /// Does not survive bundling: `current_dir()` is `/` for a `.app` launched
    /// from Finder, so a shipped build falls back to on-device whisper.
    fn fill_keys_from_env(&mut self) {
        for (var, field) in [
            ("GROQ_API_KEY", &mut self.groq_api_key),
            ("COMPOSIO_API_KEY", &mut self.composio_api_key),
            ("GEMINI_API_KEY", &mut self.gemini_api_key),
            ("CEREBRAS_API_KEY", &mut self.cerebras_api_key),
            ("NVIDIA_API_KEY", &mut self.nvidia_api_key),
            ("DEEPGRAM_API_KEY", &mut self.deepgram_api_key),
        ] {
            if !field.trim().is_empty() {
                continue;
            }
            if let Some(value) = env_or_dotenv(var) {
                *field = value;
            }
        }
    }

    /// Appends a vocabulary term, returning whether it was new. Case-insensitive
    /// — the same hint twice only lengthens every prompt — and the first
    /// spelling wins. `to_lowercase` not `eq_ignore_ascii_case`: "Café" lives
    /// here, and the ASCII comparison would store it twice.
    pub fn add_vocabulary_term(&mut self, term: &str) -> bool {
        let term = term.trim();
        if term.is_empty() {
            return false;
        }
        let folded = term.to_lowercase();
        // ponytail: linear scan — the list is a handful of words the user
        // typed, and a HashSet is the upgrade path if it ever isn't.
        if self
            .vocabulary
            .iter()
            .any(|existing| existing.trim().to_lowercase() == folded)
        {
            return false;
        }
        self.vocabulary.push(term.to_string());
        true
    }

    /// Adds every term in `text`, returning how many were new. The source is
    /// the clipboard, so it splits on newlines/commas/semicolons and bounds
    /// length by [`MAX_TERM_CHARS`] — otherwise a copied paragraph lands whole
    /// and is re-sent with every later cleanup request.
    pub fn add_vocabulary_terms(&mut self, text: &str) -> usize {
        text.split(['\n', ',', ';'])
            .filter(|t| {
                let t = t.trim();
                !t.is_empty() && t.chars().count() <= MAX_TERM_CHARS
            })
            .filter(|t| self.add_vocabulary_term(t))
            .count()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        let dir = path.parent().context("settings path has no parent")?;
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

        // Temp + rename: a crash mid-write must not truncate the API keys.
        let temp = path.with_extension("tmp");
        fs::write(&temp, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", temp.display()))?;
        restrict_permissions(&temp)?;
        fs::rename(&temp, &path).context("replacing settings file")?;
        Ok(())
    }
}

/// A var from the process environment, else the first `.env` found walking up
/// from the working directory, else one bundled beside the app. No dotenv
/// crate: three keys, `KEY=value` lines.
fn env_or_dotenv(var: &str) -> Option<String> {
    if let Ok(value) = std::env::var(var) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Ok(mut dir) = std::env::current_dir() {
        loop {
            let candidate = dir.join(".env");
            if let Ok(contents) = fs::read_to_string(&candidate) {
                if let Some(value) = parse_dotenv_value(&contents, var) {
                    return Some(value);
                }
            }
            if !dir.pop() {
                break;
            }
        }
    }

    // A `.app` launched from Finder has `current_dir()` of `/`, so the walk
    // above finds nothing and every key stays empty — which reads as "agent
    // mode is unavailable" rather than as a packaging problem. A `.env` copied
    // into the bundle's Resources is the fallback.
    //
    // ponytail: keys in the bundle are readable by anyone with the .app. Fine
    // for a build you hand to yourself or a demo machine; a public release
    // wants Keychain and a real BYOK settings pane instead.
    // Tauri keeps the declared folder name, so a `resources/*` bundle rule
    // lands one level below Resources itself; both are tried.
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    for candidate in ["../Resources/resources/.env", "../Resources/.env"] {
        if let Ok(contents) = fs::read_to_string(dir.join(candidate)) {
            if let Some(value) = parse_dotenv_value(&contents, var) {
                return Some(value);
            }
        }
    }
    None
}

/// Pulls `var` out of `.env` contents, trimming surrounding quotes/whitespace.
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

/// Owner-only (0600) on Unix. Windows' per-user profile is already ACL'd.
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
    fn rewriting_a_selection_is_on_by_default() {
        assert!(Settings::default().command_mode_enabled);
    }

    #[test]
    fn vocabulary_terms_dedupe_ignoring_case() {
        let mut settings = Settings::default();
        assert!(settings.add_vocabulary_term("Tauri"));
        assert!(!settings.add_vocabulary_term("tauri"));
        assert!(!settings.add_vocabulary_term("  TAURI \n"), "trimmed first");
        assert!(!settings.add_vocabulary_term("   "), "blank is not a term");
        assert!(settings.add_vocabulary_term("Café"));
        assert!(
            !settings.add_vocabulary_term("café"),
            "case folding must be Unicode, not ASCII"
        );
        assert_eq!(settings.vocabulary, ["Tauri", "Café"]);
    }

    /// Guards: "Tauri, Specta" arriving as one term, and a copied paragraph
    /// arriving whole.
    #[test]
    fn clipboard_text_is_split_into_terms_and_long_ones_are_dropped() {
        let mut settings = Settings::default();

        assert_eq!(
            settings.add_vocabulary_terms("Tauri, Specta; Rust\nserde"),
            4
        );
        assert_eq!(settings.vocabulary, ["Tauri", "Specta", "Rust", "serde"]);

        assert_eq!(settings.add_vocabulary_terms("tauri, SPECTA"), 0);

        let paragraph = "a".repeat(MAX_TERM_CHARS + 1);
        assert_eq!(settings.add_vocabulary_terms(&paragraph), 0);
        assert_eq!(
            settings.add_vocabulary_terms(&"b".repeat(MAX_TERM_CHARS)),
            1
        );
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

    #[test]
    fn partial_json_keeps_defaults_for_missing_fields() {
        let parsed: Settings =
            serde_json::from_str(r#"{"groqApiKey":"gsk_x","unknownField":42}"#).unwrap();
        assert_eq!(parsed.groq_api_key, "gsk_x");
        assert!(parsed.play_sounds, "missing field falls back to default");
    }

    #[test]
    fn corrupt_json_falls_back_to_defaults() {
        let parsed: Result<Settings, _> = serde_json::from_str("{not json");
        assert!(parsed.is_err(), "sanity: this really is invalid");
    }
}

#[cfg(test)]
mod rename_tests {
    use super::*;

    /// Losing the `command` → `rewrite` migration does not fail to parse: the
    /// hotkey quietly stops working, with nothing logged.
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

    /// A retired `overlayStyle` must cost that one field and nothing else. The
    /// failure guarded is the API keys in the same document, which a hard parse
    /// error would reset.
    #[test]
    fn a_retired_overlay_style_falls_back_without_resetting_the_file() {
        let stale = Settings::from_json(r#"{ "overlayStyle": "pill", "groqApiKey": "keep-me" }"#)
            .expect("a retired style must not fail the document");
        assert_eq!(stale.overlay_style, Settings::default().overlay_style);
        assert_eq!(
            stale.groq_api_key, "keep-me",
            "the rest of the file survives"
        );

        let glass =
            Settings::from_json(r#"{ "overlayStyle": "liquidGlass", "groqApiKey": "keep-me" }"#)
                .expect("a retired style must not fail the document");
        assert_eq!(glass.overlay_style, Settings::default().overlay_style);
        assert_eq!(glass.groq_api_key, "keep-me");

        for style in [OverlayStyle::DropPill, OverlayStyle::Wings] {
            let saved = serde_json::to_string(&style).expect("must serialize");
            let reloaded = Settings::from_json(&format!(r#"{{ "overlayStyle": {saved} }}"#))
                .expect("a current style must parse");
            assert_eq!(reloaded.overlay_style, style);
        }
    }

    /// The widgets look is a choice the user made: it must survive a restart,
    /// an old file with no `notchStyle` must open Calm, and a value from a
    /// newer build must cost that one field, not the keys beside it.
    #[test]
    fn the_notch_style_defaults_to_calm_and_survives_a_restart() {
        assert_eq!(Settings::default().notch_style, NotchStyle::Calm);
        let old = Settings::from_json(r#"{ "groqApiKey": "keep-me" }"#).expect("old file");
        assert_eq!(old.notch_style, NotchStyle::Calm);

        let saved = serde_json::to_string(&NotchStyle::Widgets).expect("must serialize");
        assert_eq!(saved, r#""widgets""#);
        let reloaded = Settings::from_json(&format!(r#"{{ "notchStyle": {saved} }}"#))
            .expect("settings must parse");
        assert_eq!(reloaded.notch_style, NotchStyle::Widgets);

        let future = Settings::from_json(r#"{ "notchStyle": "alive", "groqApiKey": "keep-me" }"#)
            .expect("an unknown style must not fail the document");
        assert_eq!(future.notch_style, NotchStyle::Calm);
        assert_eq!(future.groq_api_key, "keep-me");
    }

    /// The pill serialized as `"pill"`, the exact string the loader rewrote, so
    /// it reverted on every launch. `dropPill` cannot be shadowed.
    #[test]
    fn the_drop_down_pill_survives_a_restart() {
        let saved = serde_json::to_string(&OverlayStyle::DropPill).expect("must serialize");
        assert_eq!(
            saved, r#""dropPill""#,
            "must not collide with the legacy name"
        );

        let reloaded = Settings::from_json(&format!(r#"{{ "overlayStyle": {saved} }}"#))
            .expect("settings must parse");
        assert_eq!(reloaded.overlay_style, OverlayStyle::DropPill);
    }

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
