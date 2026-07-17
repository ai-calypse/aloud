//! The dictation loop: hotkey → record → transcribe → clean up → paste.
//!
//! This is the app's core. It replaces the equivalent flow in the Swift
//! `AppState`, but only orchestrates: every step lives in a crate that is
//! testable without launching the app.

use aloud_store::{History, Settings, Stats};
use aloud_transcribe::{cloud::CloudTranscriber, local::LocalTranscriber, Engine, Transcriber};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::time::Duration;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter};

/// How long the pill stays up after a dictation finishes.
const OVERLAY_LINGER: Duration = Duration::from_millis(900);

/// What the UI reflects. Mirrors the Swift overlay's phases.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Idle,
    Recording,
    Transcribing,
    /// Text was pasted.
    Done,
    Error,
}

/// Whether a dictation actually produced text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Pasted,
    /// Answered in the Ask panel rather than pasted.
    Asked,
    /// Silence or filler only — a normal result, not a failure.
    NoSpeech,
}

/// In-flight dictation state, owned by the app and shared with the hotkey
/// thread.
#[derive(Default)]
pub struct Session {
    recorder: Option<aloud_audio::Recorder>,
    /// The last transcript, for "paste again".
    last_transcript: Option<String>,
    /// Text selected when a command-mode session began.
    selection: Option<String>,
    started_at: Option<std::time::Instant>,
}

pub struct Pipeline {
    app: AppHandle,
    session: Mutex<Session>,
    settings: Arc<Mutex<Settings>>,
    history: Mutex<History>,
    stats: Mutex<Stats>,
    /// Loaded lazily: the model costs ~200ms and memory, so only when the
    /// local engine is actually used.
    local: Mutex<Option<Arc<dyn Transcriber>>>,
}

impl Pipeline {
    pub fn new(app: AppHandle, settings: Arc<Mutex<Settings>>) -> Result<Self> {
        let history_path = aloud_notes::data_dir()?.join("history.sqlite");
        Ok(Self {
            app,
            session: Mutex::new(Session::default()),
            settings,
            history: Mutex::new(History::open(&history_path)?),
            stats: Mutex::new(Stats::default()),
            local: Mutex::new(None),
        })
    }

    /// Publishes the phase and drives the overlay's visibility with it, so
    /// the pill can never be left on screen after a session ends.
    fn emit(&self, phase: Phase) {
        if let Err(e) = self.app.emit("dictation-phase", phase) {
            tracing::warn!(%e, "could not emit phase");
        }

        match phase {
            Phase::Recording | Phase::Transcribing => crate::overlay::show(&self.app, phase),
            // Leave the result on screen briefly, then dismiss.
            Phase::Done | Phase::Error => {
                let app = self.app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(OVERLAY_LINGER).await;
                    crate::overlay::hide(&app);
                });
            }
            Phase::Idle => crate::overlay::hide(&self.app),
        }
    }

    fn settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Starts capturing. Idempotent: a second call while recording is a no-op.
    pub fn start(&self) -> Result<()> {
        let mut session = self.session.lock().unwrap();
        if session.recorder.is_some() {
            return Ok(());
        }

        let settings = self.settings();
        let recorder = aloud_audio::Recorder::start(settings.selected_microphone.as_deref(), None)
            .context("starting recorder")?;

        session.recorder = Some(recorder);
        session.started_at = Some(std::time::Instant::now());
        drop(session);

        self.emit(Phase::Recording);
        Ok(())
    }

    /// Stops capturing, transcribes, cleans up and pastes.
    pub async fn stop_and_paste(&self) -> Result<()> {
        let (recorder, started_at) = {
            let mut session = self.session.lock().unwrap();
            (session.recorder.take(), session.started_at.take())
        };
        let Some(recorder) = recorder else {
            return Ok(()); // not recording
        };

        self.emit(Phase::Transcribing);
        let result = self.transcribe_and_paste(recorder, started_at).await;

        match &result {
            // Silence is a normal outcome, but reporting "Pasted" when nothing
            // was pasted would be a lie: go back to Idle instead.
            Ok(Outcome::Pasted) => self.emit(Phase::Done),
            // The Ask panel is the feedback; the pill would just cover it.
            Ok(Outcome::Asked) | Ok(Outcome::NoSpeech) => self.emit(Phase::Idle),
            Err(e) => {
                tracing::error!(%e, "dictation failed");
                self.emit(Phase::Error);
            }
        }
        result.map(|_| ())
    }

    async fn transcribe_and_paste(
        &self,
        recorder: aloud_audio::Recorder,
        started_at: Option<std::time::Instant>,
    ) -> Result<Outcome> {
        let wav = temp_wav_path();
        recorder.stop_to_wav(&wav)?;

        let settings = self.settings();
        let transcriber = self.transcriber(&settings).await?;
        let raw = transcriber.transcribe(&wav).await?;
        let _ = std::fs::remove_file(&wav);

        // Whisper returns nothing for silence, and the cleanup prompt maps
        // filler-only speech to its EMPTY sentinel. Neither is an error.
        if raw.trim().is_empty() {
            tracing::info!("no speech detected");
            return Ok(Outcome::NoSpeech);
        }

        // "Hey flow, what does this mean?" is a question, not dictation.
        if let Some(question) = aloud_llm::ask::question_from_trigger(&raw) {
            tracing::info!(%question, "spoken ask trigger");
            crate::ask::answer(&self.app, question, &settings).await?;
            return Ok(Outcome::Asked);
        }

        // Apply the focused app's rule (if any) to a working copy, so every
        // downstream step just reads the effective settings. History still
        // records against the real config; only cleanup behaviour changes.
        let effective = apply_app_rule(settings.clone());

        let final_text = self.post_process(&raw, &effective).await;
        if final_text.trim().is_empty() {
            return Ok(Outcome::NoSpeech);
        }

        aloud_paste::paste_text(&final_text, false)?;
        self.record(&raw, &final_text, started_at, &settings);
        Ok(Outcome::Pasted)
    }

    /// Cleans up the transcript, falling back to the raw text if the LLM is
    /// unavailable: pasting the unpolished words beats pasting nothing. Voice
    /// macros expand last, on whichever text we settled on.
    async fn post_process(&self, raw: &str, settings: &Settings) -> String {
        let cleaned = if !settings.post_processing_enabled || settings.groq_api_key.trim().is_empty()
        {
            raw.to_string()
        } else {
            match self.clean_up(raw, settings).await {
                Ok(text) => text,
                Err(e) => {
                    tracing::warn!(%e, "post-processing failed; pasting raw transcript");
                    raw.to_string()
                }
            }
        };

        // Expand shorthand after cleanup, so the model's spacing/casing fixes
        // land before a trigger is matched, and so macros work even with
        // post-processing off.
        if settings.macros.is_empty() {
            cleaned
        } else {
            aloud_store::macros::expand(&cleaned, &settings.macros)
        }
    }

    async fn clean_up(&self, raw: &str, settings: &Settings) -> Result<String> {
        let client = aloud_llm::LlmClient::new(&settings.groq_api_key, Some(&settings.base_url))?;

        let system = if settings.custom_system_prompt.trim().is_empty() {
            aloud_llm::prompts::CLEANUP.to_string()
        } else {
            settings.custom_system_prompt.clone()
        };

        let mut user = raw.to_string();
        if !settings.vocabulary.is_empty() {
            user = format!("VOCABULARY: {}\n\n{user}", settings.vocabulary.join(", "));
        }

        let messages = [
            aloud_llm::Message::system(system),
            aloud_llm::Message::user(user),
        ];
        let cleaned = client
            .complete_with_fallback(
                &settings.post_processing_model,
                Some(settings.fallback_model.as_str()).filter(|f| !f.trim().is_empty()),
                &messages,
                Some(0.0), // deterministic: this is cleanup, not writing
            )
            .await?;

        // The prompt specifies this sentinel for filler-only input.
        if cleaned.trim() == aloud_llm::EMPTY_SENTINEL {
            return Ok(String::new());
        }
        Ok(cleaned)
    }

    /// The configured engine. The local model is cached after first load.
    async fn transcriber(&self, settings: &Settings) -> Result<Arc<dyn Transcriber>> {
        // The local model is cached on self, since loading it costs ~200ms and
        // memory. The cloud client is cheap, so it is never cached.
        if settings.engine == Engine::Local {
            if let Some(local) = self.local.lock().unwrap().clone() {
                return Ok(local);
            }
        }
        let transcriber = build_transcriber(settings).await?;
        if settings.engine == Engine::Local {
            *self.local.lock().unwrap() = Some(transcriber.clone());
        }
        Ok(transcriber)
    }

    /// Records the dictation for history and stats. Never fails the paste:
    /// the text is already in the user's document.
    fn record(
        &self,
        raw: &str,
        final_text: &str,
        started_at: Option<std::time::Instant>,
        settings: &Settings,
    ) {
        let duration_ms = started_at.map_or(0, |s| s.elapsed().as_millis() as i64);

        if let Ok(mut session) = self.session.lock() {
            session.last_transcript = Some(final_text.to_string());
        }
        if let Ok(history) = self.history.lock() {
            let now = chrono::Utc::now().timestamp();
            if let Err(e) = history.insert(
                now,
                raw,
                final_text,
                None,
                duration_ms,
                settings.history_retention,
            ) {
                tracing::warn!(%e, "could not record history");
            }
        }
        if let Ok(mut stats) = self.stats.lock() {
            let words = Stats::word_count(final_text);
            stats.record_session(
                words,
                Stats::estimated_spoken_seconds(words),
                chrono::Local::now(),
            );
        }
    }

    /// Reads the current selection, so command mode can transform it.
    ///
    /// Called on the *leading* edge of the command hotkey, before recording:
    /// starting dictation moves focus and loses the selection.
    pub fn capture_selection(&self) {
        match aloud_selection::selected_text() {
            Ok(text) if !text.trim().is_empty() => {
                if let Ok(mut session) = self.session.lock() {
                    session.selection = Some(text);
                }
            }
            Ok(_) => tracing::info!("command mode: nothing selected"),
            Err(e) => tracing::warn!(%e, "could not read selection"),
        }
    }

    /// Stops capturing, transforms the captured selection per the spoken
    /// command, and pastes the result over it.
    pub async fn stop_and_command(&self) -> Result<()> {
        let (recorder, selection) = {
            let mut session = self.session.lock().unwrap();
            (session.recorder.take(), session.selection.take())
        };
        let Some(recorder) = recorder else {
            return Ok(());
        };
        let Some(selection) = selection else {
            // No selection was captured; treat the utterance as plain dictation
            // rather than doing nothing.
            recorder.stop_to_wav(&temp_wav_path()).ok();
            self.emit(Phase::Idle);
            return Ok(());
        };

        self.emit(Phase::Transcribing);
        let result = self.transcribe_and_command(recorder, &selection).await;
        self.emit(if result.is_ok() {
            Phase::Done
        } else {
            Phase::Error
        });
        result
    }

    async fn transcribe_and_command(
        &self,
        recorder: aloud_audio::Recorder,
        selection: &str,
    ) -> Result<()> {
        let wav = temp_wav_path();
        recorder.stop_to_wav(&wav)?;

        let settings = self.settings();
        let command = self.transcriber(&settings).await?.transcribe(&wav).await?;
        let _ = std::fs::remove_file(&wav);

        if command.trim().is_empty() {
            return Ok(()); // no command spoken; leave the selection alone
        }

        let transformed = self.transform(selection, &command, &settings).await?;
        // Paste over the still-selected text, replacing it.
        aloud_paste::paste_text(&transformed, false)?;
        Ok(())
    }

    /// Runs the command-mode transform. Ported from `commandModeSystemPrompt`.
    async fn transform(
        &self,
        selection: &str,
        command: &str,
        settings: &Settings,
    ) -> Result<String> {
        if settings.groq_api_key.trim().is_empty() {
            bail!("Command mode needs an API key — add one in Settings");
        }
        let client = aloud_llm::LlmClient::new(&settings.groq_api_key, Some(&settings.base_url))?;
        let messages = [
            aloud_llm::Message::system(aloud_llm::prompts::COMMAND),
            aloud_llm::Message::user(format!(
                "SELECTED_TEXT:\n{selection}\n\nVOICE_COMMAND:\n{command}"
            )),
        ];
        client
            .complete_with_fallback(
                &settings.post_processing_model,
                Some(settings.fallback_model.as_str()).filter(|f| !f.trim().is_empty()),
                &messages,
                Some(0.2),
            )
            .await
    }

    /// Stops capturing and answers the question, rather than pasting it.
    ///
    /// The Ask hotkey means the whole utterance is a question, so no spoken
    /// trigger phrase is needed.
    pub async fn stop_and_ask(&self) -> Result<()> {
        let recorder = self.session.lock().unwrap().recorder.take();
        let Some(recorder) = recorder else {
            return Ok(());
        };

        self.emit(Phase::Transcribing);
        let result = self.transcribe_and_ask(recorder).await;
        self.emit(if result.is_ok() {
            Phase::Idle
        } else {
            Phase::Error
        });
        result
    }

    async fn transcribe_and_ask(&self, recorder: aloud_audio::Recorder) -> Result<()> {
        let wav = temp_wav_path();
        recorder.stop_to_wav(&wav)?;

        let settings = self.settings();
        let question = self.transcriber(&settings).await?.transcribe(&wav).await?;
        let _ = std::fs::remove_file(&wav);

        if question.trim().is_empty() {
            return Ok(());
        }
        // Tolerate a trigger phrase said out of habit while holding the key.
        let question = aloud_llm::ask::question_from_trigger(&question).unwrap_or(question);
        crate::ask::answer(&self.app, question, &settings).await
    }

    /// Stops capturing and runs the spoken command as an agent action,
    /// rather than pasting it. Like ask, the whole utterance is the command,
    /// so no spoken trigger phrase is needed.
    pub async fn stop_and_agent(&self) -> Result<()> {
        let recorder = self.session.lock().unwrap().recorder.take();
        let Some(recorder) = recorder else {
            return Ok(());
        };

        self.emit(Phase::Transcribing);
        let result = self.transcribe_and_agent(recorder).await;
        self.emit(if result.is_ok() {
            Phase::Idle
        } else {
            Phase::Error
        });
        result
    }

    async fn transcribe_and_agent(&self, recorder: aloud_audio::Recorder) -> Result<()> {
        let wav = temp_wav_path();
        recorder.stop_to_wav(&wav)?;

        let settings = self.settings();
        let command = self.transcriber(&settings).await?.transcribe(&wav).await?;
        let _ = std::fs::remove_file(&wav);

        if command.trim().is_empty() {
            return Ok(());
        }
        // Tolerate a trigger phrase said out of habit while holding the key.
        let command =
            aloud_agent::command_from_trigger(&command).unwrap_or(command);
        crate::agent::run(&self.app, command, &settings).await
    }

    /// Cancels an in-flight recording, discarding the audio.
    pub fn cancel(&self) {
        if let Ok(mut session) = self.session.lock() {
            // Dropping the recorder stops capture; the samples go with it.
            session.recorder = None;
            session.started_at = None;
        }
        self.emit(Phase::Idle);
    }

    /// Re-pastes the last transcript.
    pub fn paste_again(&self) -> Result<()> {
        let last = self
            .session
            .lock()
            .ok()
            .and_then(|s| s.last_transcript.clone());
        match last {
            Some(text) => aloud_paste::paste_text(&text, false),
            None => Ok(()),
        }
    }

    pub fn stats(&self) -> Stats {
        self.stats.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn history(&self, limit: usize) -> Result<Vec<aloud_store::history::Entry>> {
        self.history.lock().unwrap().recent(limit)
    }
}

/// Builds the configured transcription engine. Shared by the pipeline and
/// meeting notes; the pipeline caches the local model, this does not.
pub async fn build_transcriber(settings: &Settings) -> Result<Arc<dyn Transcriber>> {
    match settings.engine {
        Engine::Cloud => {
            if settings.groq_api_key.trim().is_empty() {
                bail!("cloud transcription needs an API key");
            }
            Ok(Arc::new(CloudTranscriber::new(
                &settings.groq_api_key,
                Some(&settings.base_url),
                Some(&settings.transcription_model),
                Some(&settings.language),
            )?))
        }
        Engine::Local => {
            let dir = aloud_transcribe::local::model_dir()?;
            let model = aloud_transcribe::download::ensure_model(
                &dir,
                aloud_transcribe::local::DEFAULT_MODEL,
                |p| tracing::debug!(progress = p, "downloading model"),
            )
            .await?;
            let language = Some(settings.language.as_str()).filter(|l| !l.is_empty());
            Ok(Arc::new(LocalTranscriber::new(&model, language)?))
        }
    }
}

/// Overrides `settings` with the rule for the currently focused app, if one
/// matches. Reads the frontmost app once; a None app or no matching rule
/// leaves the settings untouched.
fn apply_app_rule(mut settings: Settings) -> Settings {
    if settings.app_rules.is_empty() {
        return settings;
    }
    let app = aloud_selection::frontmost_app();
    let Some(rule) = aloud_store::rules::matching(&settings.app_rules, app.as_deref()) else {
        return settings;
    };
    tracing::info!(app = ?app, matcher = %rule.app_matches, "applying app rule");

    if let Some(enabled) = rule.post_processing_enabled {
        settings.post_processing_enabled = enabled;
    }
    if !rule.custom_prompt.trim().is_empty() {
        settings.custom_system_prompt = rule.custom_prompt.clone();
    }
    settings
}

/// A unique temp path per dictation, so overlapping sessions cannot collide.
fn temp_wav_path() -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!("aloud-{unique}.wav"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_paths_are_unique() {
        assert_ne!(temp_wav_path(), temp_wav_path());
    }
}
