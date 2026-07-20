//! The dictation loop: hotkey → record → transcribe → clean up → paste.
//!
//! This is the app's core. It replaces the equivalent flow in the Swift
//! `AppState`, but only orchestrates: every step lives in a crate that is
//! testable without launching the app.

use aloud_store::{History, Settings, Stats};
use aloud_transcribe::{cloud::CloudTranscriber, local::LocalTranscriber, Engine, Transcriber};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::AppHandle;
use tauri_specta::Event;
use tracing::Instrument;

/// How long the pill stays up after a dictation finishes.
const OVERLAY_LINGER: Duration = Duration::from_millis(900);
/// How long a failure stays up. Long enough to actually read: 900ms was tuned
/// for "Pasted", and left a rate-limit message unreadable.
const ERROR_LINGER: Duration = Duration::from_secs(4);
/// Correlates every stage of one dictation. A counter rather than a UUID: it
/// is only ever read back on this machine, and it sorts by time for free —
/// which is only true because the store seeds it from disk on open.
use aloud_store::timings::next_utterance_id;

/// How often the input level is pushed to the overlay. 20 Hz is smooth to the
/// eye without flooding the event channel.
const LEVEL_INTERVAL: Duration = Duration::from_millis(50);

/// What the UI reflects. Mirrors the Swift overlay's phases.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "dictation-phase")]
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
    /// Escape landed while the work was in flight.
    Cancelled,
}

/// In-flight dictation state, owned by the app and shared with the hotkey
/// thread.
#[derive(Default)]
pub struct Session {
    recorder: Option<aloud_audio::Recorder>,
    /// The last transcript, for "paste again".
    last_transcript: Option<String>,
    /// The in-flight read of the text selected when a command-mode session
    /// began. A handle rather than a value because reading it can take a
    /// clipboard round-trip, which must not happen on the hotkey thread.
    selection: Option<tauri::async_runtime::JoinHandle<Option<String>>>,
    started_at: Option<std::time::Instant>,
    /// A start has been reserved but the delay has not elapsed, so `recorder`
    /// is still None. Counts as recording everywhere, or a second press during
    /// the delay would queue a second capture.
    pending_start: bool,
    /// Bumped by every start and every stop. A delayed start compares this
    /// against the value it captured and gives up if it moved — which is what
    /// makes a tap shorter than the delay stop cleanly rather than starting a
    /// capture that nothing is left to stop.
    start_generation: u64,
}

impl Session {
    /// Claims the session for a start and returns the generation a delayed
    /// start must still see when it wakes.
    fn reserve_start(&mut self) -> u64 {
        self.start_generation = self.start_generation.wrapping_add(1);
        self.start_generation
    }

    /// Abandons any reserved start. Every stop path calls this, so a release
    /// arriving inside the delay wins over the start it is racing.
    fn invalidate_start(&mut self) {
        self.pending_start = false;
        self.start_generation = self.start_generation.wrapping_add(1);
    }

    /// Whether a delayed start holding `generation` may still open the mic.
    fn may_begin(&self, generation: u64) -> bool {
        self.start_generation == generation && self.recorder.is_none()
    }

    /// A capture is running, or reserved and about to.
    fn is_active(&self) -> bool {
        self.recorder.is_some() || self.pending_start
    }
}

pub struct Pipeline {
    app: AppHandle,
    session: Mutex<Session>,
    settings: Arc<Mutex<Settings>>,
    history: Mutex<History>,
    stats: Mutex<Stats>,
    /// The last phase emitted, so the tray menu can show what the app is doing
    /// without the frontend having to be awake to tell it.
    phase: Mutex<Phase>,
    /// Loaded lazily: the model costs ~200ms and memory, so only when the
    /// local engine is actually used.
    local: Mutex<Option<Arc<dyn Transcriber>>>,
    /// True from the moment a session starts until it settles. Read by the
    /// hotkey thread to decide whether Escape is ours to swallow.
    busy: Arc<std::sync::atomic::AtomicBool>,
    /// Fired by [`Pipeline::cancel`] to abandon an in-flight session. Racing
    /// this against the work drops the transcribe/LLM future, which aborts the
    /// HTTP request — dropping the recorder alone left the request running and
    /// the pill up for as long as 80s.
    cancelled: tokio::sync::Notify,
    /// Bumped by every cancel. `notify_waiters` only wakes waiters that have
    /// already registered, so an Escape arriving between the phase change and
    /// the `select!` would be lost and the transcript would paste anyway. The
    /// counter closes that window: the work checks it before using its result.
    cancel_generation: std::sync::atomic::AtomicU64,
}

impl Pipeline {
    pub fn new(app: AppHandle, settings: Arc<Mutex<Settings>>) -> Result<Self> {
        let history_path = aloud_notes::data_dir()?.join("history.sqlite");
        Ok(Self {
            app,
            session: Mutex::new(Session::default()),
            settings,
            history: Mutex::new(History::open(&history_path)?),
            // Lifetime stats outlive the process; a fresh Stats here silently
            // zeroed the user's words, sessions, and streak on every launch.
            stats: Mutex::new(Stats::load()),
            phase: Mutex::new(Phase::Idle),
            local: Mutex::new(None),
            busy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cancelled: tokio::sync::Notify::new(),
            cancel_generation: std::sync::atomic::AtomicU64::new(0),
        })
    }

    fn generation(&self) -> u64 {
        self.cancel_generation
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Shared with the hotkey listener so Escape knows a session is running.
    pub fn busy_flag(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.busy.clone()
    }

    /// Publishes the phase and drives the overlay's visibility with it, so
    /// the pill can never be left on screen after a session ends.
    fn emit(&self, phase: Phase) {
        if let Ok(mut current) = self.phase.lock() {
            *current = phase;
        }
        // Escape is only ours to swallow while something is actually running.
        self.busy.store(
            matches!(phase, Phase::Recording | Phase::Transcribing),
            std::sync::atomic::Ordering::Relaxed,
        );
        // The tray shows the phase and the running totals, and it is visible
        // when the main window is not.
        crate::tray::refresh(&self.app);

        if let Err(e) = phase.emit(&self.app) {
            tracing::warn!(%e, "could not emit phase");
        }

        // Chime on the phase transitions voice-dict sounded: Tink on record,
        // Pop when transcription begins, Basso on error. Done/Idle stay silent.
        let chime = match phase {
            Phase::Recording => Some(crate::sound::Chime::Start),
            Phase::Transcribing => Some(crate::sound::Chime::Stop),
            Phase::Error => Some(crate::sound::Chime::Error),
            Phase::Done | Phase::Idle => None,
        };
        if let Some(chime) = chime {
            let settings = self.settings();
            crate::sound::play(chime, settings.play_sounds, settings.sound_volume);
        }

        match phase {
            Phase::Recording | Phase::Transcribing => crate::overlay::show(&self.app, phase),
            // Leave the result on screen briefly, then dismiss.
            Phase::Done | Phase::Error => {
                let app = self.app.clone();
                let linger = if matches!(phase, Phase::Error) {
                    ERROR_LINGER
                } else {
                    OVERLAY_LINGER
                };
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(linger).await;
                    crate::overlay::hide(&app);
                });
            }
            Phase::Idle => crate::overlay::hide(&self.app),
        }
    }

    /// Reports a failure to the user, not just to the log. The pill showed a
    /// bare "Failed" for every error while the real cause went to stderr.
    fn emit_error(&self, error: &anyhow::Error) {
        crate::overlay::send_error(&self.app, &user_message(error));
        self.emit(Phase::Error);
    }

    fn settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// The last phase emitted, for the tray menu's status line.
    pub fn phase(&self) -> Phase {
        self.phase.lock().map(|p| *p).unwrap_or(Phase::Idle)
    }

    /// The last transcript, if any — the tray greys out "Paste Again" without
    /// one and shows a preview with it.
    pub fn last_transcript(&self) -> Option<String> {
        self.session
            .lock()
            .ok()
            .and_then(|s| s.last_transcript.clone())
    }

    /// Whether a capture session is in flight. Drives the tap-to-toggle
    /// hotkey, which has no release edge to tell start from stop.
    ///
    /// A reserved-but-not-yet-started capture counts: during the start delay
    /// there is no recorder yet, and reporting "not recording" would let the
    /// toggle hotkey queue a second one.
    pub fn is_recording(&self) -> bool {
        self.session.lock().map(|s| s.is_active()).unwrap_or(false)
    }

    /// Takes the recorder for a stop path, and invalidates any pending start.
    ///
    /// Every stop goes through here. A stop arriving *during* the start delay
    /// finds no recorder and would otherwise return "not recording" and leave
    /// the delayed start to fire into a session nothing is waiting on — a
    /// capture that runs until the next press, with the pill up the whole time.
    fn take_recorder(&self) -> (Option<aloud_audio::Recorder>, Option<std::time::Instant>) {
        let mut session = self.session.lock().unwrap();
        session.invalidate_start();
        let taken = (session.recorder.take(), session.started_at.take());
        drop(session);

        // Unconditional, and not gated on the setting: if the user turned
        // muting off mid-dictation, the volume is still down and the saved
        // level is still on disk. restore() is a no-op when nothing is saved,
        // so calling it always is both cheaper and safer than deciding.
        if let Err(e) = aloud_volume::restore() {
            tracing::warn!(%e, "could not restore the system volume");
        }
        taken
    }

    /// Starts capturing. Idempotent: a second call while recording is a no-op.
    ///
    /// With a non-zero `shortcut_start_delay_ms` this reserves the slot and
    /// returns immediately, starting capture from the runtime once the delay
    /// elapses. It must return immediately: this runs on the CGEventTap
    /// callback thread, where sleeping stalls every keystroke on the machine
    /// and can trip `kCGEventTapDisabledByTimeout`, which kills the listener
    /// silently. That is what PR #60 got wrong.
    pub fn start(self: &Arc<Self>) -> Result<()> {
        let delay = self.settings().shortcut_start_delay_ms;
        let mut session = self.session.lock().unwrap();
        if session.is_active() {
            return Ok(());
        }
        let generation = session.reserve_start();

        if delay == 0 {
            return self.begin_capture(session, generation);
        }

        session.pending_start = true;
        drop(session);

        // The pill goes up now rather than after the delay: the user pressed
        // the key, and silence until capture begins reads as a dropped hotkey.
        self.emit(Phase::Recording);

        let pipeline = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            let session = pipeline.session.lock().unwrap();
            // Released, cancelled, or restarted while we slept.
            if !session.may_begin(generation) {
                return;
            }
            if let Err(e) = pipeline.begin_capture(session, generation) {
                tracing::error!(%e, "could not start recording after the delay");
                pipeline.emit_error(&e);
            }
        });
        Ok(())
    }

    /// Opens the microphone and marks the session live. Takes the session guard
    /// so the caller's generation check and this cannot be raced apart.
    fn begin_capture(
        &self,
        mut session: std::sync::MutexGuard<'_, Session>,
        generation: u64,
    ) -> Result<()> {
        let settings = self.settings();
        let recorder =
            match aloud_audio::Recorder::start(settings.selected_microphone.as_deref(), None) {
                Ok(recorder) => recorder,
                Err(e) => {
                    // Leaving this set would make every later start a no-op.
                    session.pending_start = false;
                    return Err(e).context("starting recorder");
                }
            };

        // Feed the overlay's waveform. The handle stays alive only as long as
        // the recorder, so the task ends by itself when recording stops —
        // no stop flag to keep in sync with five different stop paths.
        let level = recorder.level_handle();
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            while Arc::strong_count(&level) > 1 {
                let value = f32::from_bits(level.load(std::sync::atomic::Ordering::Relaxed));
                crate::overlay::send_level(&app, value);
                tokio::time::sleep(LEVEL_INTERVAL).await;
            }
        });

        // After the mic is open, so a failed start does not leave the machine
        // silent. Never fatal: a dictation is more use than a muted one.
        if settings.mute_system_audio {
            if let Err(e) = aloud_volume::mute() {
                tracing::warn!(%e, "could not mute the system volume");
            }
        }

        session.pending_start = false;
        session.recorder = Some(recorder);
        // Timed from when audio actually starts, not from the key press, so
        // the delay is not billed to the user as speaking time.
        session.started_at = Some(std::time::Instant::now());
        debug_assert_eq!(session.start_generation, generation);
        drop(session);

        self.emit(Phase::Recording);
        Ok(())
    }

    /// Stops capturing, transcribes, cleans up and pastes.
    ///
    /// The span opens here, on the hotkey's release edge, so its duration is
    /// the one latency number the user actually feels; every stage below is a
    /// slice of it. Instrumenting only the inner calls would report a pipeline
    /// measurably faster than the app, because the clock starts before them.
    #[tracing::instrument(
        name = "utterance",
        skip_all,
        fields(
            utterance_id = next_utterance_id(),
            mode = "dictation",
            spoken_ms = tracing::field::Empty,
            outcome = tracing::field::Empty,
            ok = tracing::field::Empty,
        )
    )]
    pub async fn stop_and_paste(&self) -> Result<()> {
        let (recorder, started_at) = self.take_recorder();
        let Some(recorder) = recorder else {
            return Ok(()); // not recording
        };

        let span = tracing::Span::current();
        if let Some(start) = started_at {
            span.record("spoken_ms", start.elapsed().as_millis() as u64);
        }

        let generation = self.generation();
        self.emit(Phase::Transcribing);
        let result = tokio::select! {
            result = self.transcribe_and_paste(recorder, started_at, generation) => result,
            // Dropping the work future here is what actually aborts the
            // in-flight HTTP request.
            _ = self.cancelled.notified() => {
                tracing::info!("dictation cancelled");
                span.record("outcome", "cancelled");
                self.emit(Phase::Idle);
                return Ok(());
            }
        };

        match &result {
            Ok(outcome) => {
                span.record("outcome", format!("{outcome:?}").to_lowercase());
            }
            Err(_) => {
                span.record("outcome", "error");
                span.record("ok", false);
            }
        }

        match &result {
            // Silence is a normal outcome, but reporting "Pasted" when nothing
            // was pasted would be a lie: go back to Idle instead.
            Ok(Outcome::Pasted) => self.emit(Phase::Done),
            // The Ask panel is the feedback; the pill would just cover it.
            Ok(Outcome::Asked) | Ok(Outcome::NoSpeech) | Ok(Outcome::Cancelled) => {
                self.emit(Phase::Idle)
            }
            Err(e) => {
                tracing::error!(%e, "dictation failed");
                self.emit_error(e);
            }
        }
        result.map(|_| ())
    }

    /// Finalizes the recording and transcribes it, returning the text and the
    /// settings snapshot the rest of the stage runs against.
    ///
    /// Every mode starts this way. Written out four times it had already
    /// drifted: only the dictation copy recorded `wav_bytes`, so three of the
    /// four waterfalls were missing a stage field for no reason anyone chose.
    /// The shared shape of a stop: take the recorder, show Transcribing, run
    /// the mode's work, then settle the pill on `done` or the error.
    ///
    /// The public `stop_and_*` keep their own `#[tracing::instrument]` — the
    /// `mode` field differs per mode and is the one thing that must not be
    /// shared.
    async fn run_stage<F, Fut>(&self, done: Phase, work: F) -> Result<()>
    where
        F: FnOnce(aloud_audio::Recorder) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let (recorder, _) = self.take_recorder();
        let Some(recorder) = recorder else {
            return Ok(());
        };

        self.emit(Phase::Transcribing);
        let result = work(recorder).await;
        if result.is_err() {
            tracing::Span::current().record("ok", false);
        }
        match &result {
            Ok(()) => self.emit(done),
            Err(e) => self.emit_error(e),
        }
        result
    }

    async fn capture(&self, recorder: aloud_audio::Recorder) -> Result<(String, Settings)> {
        let wav = temp_wav_path();
        // Draining the capture buffer and writing the WAV is invisible latency
        // between the key release and the first byte the transcriber sees.
        {
            let span = tracing::info_span!("finalize_audio", wav_bytes = tracing::field::Empty);
            let _e = span.enter();
            recorder.stop_to_wav(&wav)?;
            if let Ok(meta) = std::fs::metadata(&wav) {
                span.record("wav_bytes", meta.len());
            }
        }

        let settings = self.settings();
        // The first local run pays for the model load inside this span, which
        // is right: it is time the user spends waiting for a transcript.
        let text = async {
            let transcriber = self.transcriber(&settings).await?;
            let text = transcriber.transcribe(&wav).await?;
            tracing::Span::current().record("chars", text.len());
            anyhow::Ok(text)
        }
        .instrument(tracing::info_span!(
            "transcribe",
            engine = ?settings.engine,
            chars = tracing::field::Empty,
        ))
        .await?;
        let _ = std::fs::remove_file(&wav);
        Ok((text, settings))
    }

    async fn transcribe_and_paste(
        &self,
        recorder: aloud_audio::Recorder,
        started_at: Option<std::time::Instant>,
        generation: u64,
    ) -> Result<Outcome> {
        let (raw, settings) = self.capture(recorder).await?;

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
        // Off-thread for the same reason `capture_selection` is: an
        // Accessibility read waits on the *target* app, so a busy app would
        // stall a tokio worker here.
        let app_name = tauri::async_runtime::spawn_blocking(aloud_selection::frontmost_app)
            .await
            .ok()
            .flatten();
        let effective = apply_app_rule(settings.clone(), app_name.as_deref());

        let final_text = self.post_process(&raw, &effective).await;
        if final_text.trim().is_empty() {
            return Ok(Outcome::NoSpeech);
        }

        // Last point before the text lands in someone else's app. A cancel
        // that raced the `select!` registration is only truly honoured here:
        // pasting after Escape is the one failure the user cannot undo.
        if self.generation() != generation {
            tracing::info!("cancelled before pasting");
            return Ok(Outcome::Cancelled);
        }

        {
            let _e = tracing::info_span!(
                "paste",
                target_app = app_name.as_deref().unwrap_or("unknown"),
                chars = final_text.len(),
            )
            .entered();
            aloud_paste::paste_text(&final_text, false, settings.preserve_clipboard)?;
        }
        self.record(
            &raw,
            &final_text,
            started_at,
            &settings,
            app_name.as_deref(),
        );
        Ok(Outcome::Pasted)
    }

    /// Cleans up the transcript, falling back to the raw text if the LLM is
    /// unavailable: pasting the unpolished words beats pasting nothing. Voice
    /// macros expand last, on whichever text we settled on.
    async fn post_process(&self, raw: &str, settings: &Settings) -> String {
        let cleaned =
            if !settings.post_processing_enabled || settings.groq_api_key.trim().is_empty() {
                raw.to_string()
            } else {
                let options = aloud_llm::cleanup::Options {
                    api_key: &settings.groq_api_key,
                    base_url: &settings.base_url,
                    model: &settings.post_processing_model,
                    fallback: Some(settings.fallback_model.as_str()),
                    custom_prompt: &settings.custom_system_prompt,
                    vocabulary: &settings.vocabulary,
                };
                match aloud_llm::cleanup::clean(options, raw).await {
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

    /// The configured engine. The local model is cached after first load.
    async fn transcriber(&self, settings: &Settings) -> Result<Arc<dyn Transcriber>> {
        // The local model is cached on self, since loading it costs ~200ms and
        // memory. The cloud client is cheap, so it is never cached. Keyed on
        // the *effective* engine so a cloud-to-local fallback caches too.
        let engine = effective_engine(settings);
        if engine == Engine::Local {
            if let Some(local) = self.local.lock().unwrap().clone() {
                return Ok(local);
            }
        }
        let transcriber = build_transcriber(&self.app, settings).await?;
        if engine == Engine::Local {
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
        app_name: Option<&str>,
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
                app_name,
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
            // Persist immediately: dictation is the only writer, and the app is
            // routinely quit without a clean shutdown hook.
            if let Err(e) = stats.save() {
                tracing::warn!(%e, "could not save stats");
            }
        }
    }

    /// Reads the current selection, so an utterance can rewrite it.
    ///
    /// Called on the *leading* edge of the command hotkey, before recording:
    /// starting dictation moves focus and loses the selection.
    /// Notes the selection for a plain dictation, if the OS will say what it
    /// is without being asked twice.
    ///
    /// Deliberately the quiet read: this runs on every utterance, and the
    /// clipboard fallback would synthesise a Cmd+C into the user's app each
    /// time they dictate. Where Accessibility cannot answer we simply dictate,
    /// which is what happened before selection-aware dictation existed.
    pub fn note_selection_if_any(&self) {
        // Off the hotkey thread like the rewrite path, and for the same reason:
        // an Accessibility read waits on the target app, which can be busy.
        let handle = tauri::async_runtime::spawn_blocking(aloud_selection::selected_text_quietly);
        if let Ok(mut session) = self.session.lock() {
            session.selection = Some(handle);
        }
    }

    /// Starts reading the selection, without waiting for it.
    ///
    /// Called from the hotkey thread, which must never block: a stalled event
    /// tap stops delivering key events system-wide, and macOS eventually
    /// disables it outright. Reading a selection can be slow — where the
    /// Accessibility API cannot answer, `aloud_selection` falls back to typing
    /// Cmd+C and waiting for the clipboard to settle — so it happens on a
    /// blocking worker and `stop_and_dictate` awaits the result.
    pub fn capture_selection(&self) {
        let handle =
            tauri::async_runtime::spawn_blocking(|| match aloud_selection::selected_text() {
                Ok(text) if !text.trim().is_empty() => Some(text),
                Ok(_) => {
                    tracing::info!("rewrite: nothing selected");
                    None
                }
                Err(e) => {
                    tracing::warn!(%e, "could not read selection");
                    None
                }
            });
        if let Ok(mut session) = self.session.lock() {
            session.selection = Some(handle);
        }
    }

    /// Stops capturing and either rewrites the selection or types fresh text.
    ///
    /// One stop path for both hotkeys. They differ only in the read started
    /// when the key went down: dictation asks Accessibility quietly, while the
    /// rewrite key also pays for the clipboard fallback. Whatever that read
    /// found decides the outcome here — an utterance with something selected is
    /// an instruction for changing it, and with nothing selected it is the text
    /// itself.
    ///
    /// Deliberately not instrumented: this only routes. Both branches open
    /// their own `utterance` span, so a span here would nest inside itself and
    /// report every dictation twice.
    pub async fn stop_and_dictate(&self) -> Result<()> {
        let reading = self.session.lock().unwrap().selection.take();
        let selection = match reading {
            // Started when the key went down, so by key-up it has almost
            // always finished.
            Some(handle) => handle.await.ok().flatten(),
            None => None,
        };

        match selection {
            Some(selection) => self.finish_rewrite(selection).await,
            // Nothing selected, so the utterance is dictation. This used to
            // stop the recorder and drop the audio on the floor.
            None => self.stop_and_paste().await,
        }
    }

    #[tracing::instrument(
        name = "utterance",
        skip_all,
        fields(
            utterance_id = next_utterance_id(),
            mode = "rewrite",
            ok = tracing::field::Empty,
        )
    )]
    async fn finish_rewrite(&self, selection: String) -> Result<()> {
        self.run_stage(Phase::Done, |r| self.transcribe_and_rewrite(r, &selection))
            .await
    }

    async fn transcribe_and_rewrite(
        &self,
        recorder: aloud_audio::Recorder,
        selection: &str,
    ) -> Result<()> {
        let (command, settings) = self.capture(recorder).await?;

        if command.trim().is_empty() {
            return Ok(()); // no command spoken; leave the selection alone
        }

        let transformed = self
            .transform(selection, &command, &settings)
            .instrument(tracing::info_span!(
                "transform",
                model = %settings.post_processing_model,
                selection_chars = selection.len(),
            ))
            .await?;
        {
            let _e = tracing::info_span!("paste", chars = transformed.len()).entered();
            // Paste over the still-selected text, replacing it.
            aloud_paste::paste_text(&transformed, false, self.settings().preserve_clipboard)?;
        }
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
            bail!("Rewriting a selection is unavailable — no credential is configured");
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
    #[tracing::instrument(
        name = "utterance",
        skip_all,
        fields(
            utterance_id = next_utterance_id(),
            mode = "ask",
            ok = tracing::field::Empty,
        )
    )]
    pub async fn stop_and_ask(&self) -> Result<()> {
        // Ask and Agent show their own panel; the pill just gets out of the
        // way unless there is an error worth reading.
        self.run_stage(Phase::Idle, |r| self.transcribe_and_ask(r))
            .await
    }

    async fn transcribe_and_ask(&self, recorder: aloud_audio::Recorder) -> Result<()> {
        let (question, settings) = self.capture(recorder).await?;

        if question.trim().is_empty() {
            return Ok(());
        }
        // Tolerate a trigger phrase said out of habit while holding the key.
        let question = aloud_llm::ask::question_from_trigger(&question).unwrap_or(question);
        crate::ask::answer(&self.app, question, &settings)
            .instrument(tracing::info_span!("ask_answer"))
            .await
    }

    /// Stops capturing and runs the spoken command as an agent action,
    /// rather than pasting it. Like ask, the whole utterance is the command,
    /// so no spoken trigger phrase is needed.
    #[tracing::instrument(
        name = "utterance",
        skip_all,
        fields(
            utterance_id = next_utterance_id(),
            mode = "agent",
            ok = tracing::field::Empty,
        )
    )]
    pub async fn stop_and_agent(&self) -> Result<()> {
        // Ask and Agent show their own panel; the pill just gets out of the
        // way unless there is an error worth reading.
        self.run_stage(Phase::Idle, |r| self.transcribe_and_agent(r))
            .await
    }

    async fn transcribe_and_agent(&self, recorder: aloud_audio::Recorder) -> Result<()> {
        let (command, settings) = self.capture(recorder).await?;

        if command.trim().is_empty() {
            return Ok(());
        }
        // Tolerate a trigger phrase said out of habit while holding the key.
        let command = aloud_agent::command_from_trigger(&command).unwrap_or(command);
        crate::agent::run(&self.app, command, &settings).await
    }

    /// Cancels an in-flight session, discarding the audio and abandoning any
    /// transcription or cleanup already sent.
    pub fn cancel(&self) {
        // Dropping the recorder stops capture; the samples go with it. Going
        // through take_recorder also invalidates a start still inside its
        // delay, which Escape must reach or the capture would begin anyway.
        let _ = self.take_recorder();
        // Wakes the select! in the stop_* paths. A no-op when nothing is
        // waiting, which is the case for a cancel during plain recording —
        // the emit below is what ends that one.
        self.cancel_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.cancelled.notify_waiters();
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
            Some(text) => aloud_paste::paste_text(&text, false, self.settings().preserve_clipboard),
            None => Ok(()),
        }
    }

    pub fn stats(&self) -> Stats {
        self.stats.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn history(&self, limit: usize) -> Result<Vec<aloud_store::history::Entry>> {
        // The tray builds its History submenu from this on the main thread, so
        // a poisoned lock here would panic the menu build rather than just
        // showing an empty submenu. `record` already degrades on this same
        // mutex; this was the one place that did not.
        self.history
            .lock()
            .map_err(|_| anyhow::anyhow!("history lock poisoned"))?
            .recent(limit)
    }
}

/// The engine that will actually run. Local is the floor, not just the
/// default: with no key resolvable there is no UI left to go fix it, so an
/// unconfigured cloud engine dictates on-device rather than erroring.
pub fn effective_engine(settings: &Settings) -> Engine {
    if settings.engine == Engine::Cloud && settings.groq_api_key.trim().is_empty() {
        tracing::warn!("no cloud key resolved; falling back to on-device whisper");
        return Engine::Local;
    }
    settings.engine
}

/// Builds the configured transcription engine. Shared by the pipeline and
/// meeting notes; the pipeline caches the local model, this does not.
/// `app` is only used to report model-download progress; the transcriber
/// itself has no Tauri dependency.
pub async fn build_transcriber(
    app: &AppHandle,
    settings: &Settings,
) -> Result<Arc<dyn Transcriber>> {
    match effective_engine(settings) {
        Engine::Cloud => Ok(Arc::new(CloudTranscriber::new(
            &settings.groq_api_key,
            Some(&settings.base_url),
            Some(&settings.transcription_model),
            Some(&settings.language),
        )?)),
        Engine::Local => {
            let dir = aloud_transcribe::local::model_dir()?;
            let model = aloud_transcribe::download::ensure_model(
                &dir,
                aloud_transcribe::local::DEFAULT_MODEL,
                |p| crate::overlay::send_download(app, p),
            )
            .await?;
            let language = Some(settings.language.as_str()).filter(|l| !l.is_empty());
            Ok(Arc::new(LocalTranscriber::new(&model, language)?))
        }
    }
}

/// Overrides `settings` with the rule for `app`, if one matches. A None app or
/// no matching rule leaves the settings untouched.
///
/// Takes the app name rather than reading it: the Accessibility call blocks on
/// the target app, so the caller does it off-thread and also reuses the name
/// for the history record.
fn apply_app_rule(mut settings: Settings, app: Option<&str>) -> Settings {
    if settings.app_rules.is_empty() {
        return settings;
    }
    let Some(rule) = aloud_store::rules::matching(&settings.app_rules, app) else {
        return settings;
    };
    tracing::info!(app = ?app, matcher = %rule.app_matches, "applying app rule");

    if let Some(enabled) = rule.post_processing_enabled {
        settings.post_processing_enabled = enabled;
    }
    // The rule's prompt is a *style hint*, appended to whichever base prompt is
    // in play — as in the Swift app. Replacing the base (what this used to do)
    // silently discarded every cleanup instruction, so a "keep it casual" rule
    // turned off punctuation repair too.
    if !rule.custom_prompt.trim().is_empty() {
        settings.custom_system_prompt =
            prompt_with_hint(&settings.custom_system_prompt, &rule.custom_prompt);
    }
    settings
}

/// The effective cleanup prompt for an app rule: the rule's text is a style
/// hint *appended* to whichever base prompt is in play, never a replacement.
fn prompt_with_hint(custom: &str, hint: &str) -> String {
    let base = if custom.trim().is_empty() {
        aloud_llm::prompts::CLEANUP
    } else {
        custom
    };
    format!("{}\n\n{}", base.trim_end(), hint.trim())
}

/// A failure in words the user can act on.
///
/// The pill is one truncated line, so the raw error is the wrong thing to show
/// — a rate limit arrived as `transcription failed (429 Too Many Requests):
/// {"error":{"message":"Rate limit reached for model...`, cut off long before
/// the part that matters. Everything else falls through unchanged rather than
/// being guessed at.
fn user_message(error: &anyhow::Error) -> String {
    let full = format!("{error:#}");
    if full.contains("429") || full.to_lowercase().contains("rate limit") {
        // The wait is in the headers, which are gone by here; the LLM path
        // reports its own cooldown, so keep this honest and short.
        return "Rate limited — wait a moment and try again".to_string();
    }
    if full.contains("401") || full.contains("invalid_api_key") {
        // Not a fallback case: the key is present, just rejected, so
        // `effective_engine` still picks Cloud and the next call fails too.
        // Switching to on-device is the user's only recovery.
        return "Credential rejected — switch to on-device in Settings".to_string();
    }
    if full.contains("timed out") || full.contains("operation timed out") {
        return "Timed out — check your connection".to_string();
    }
    full
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

    /// The pill shows one truncated line, so a 429 must not arrive as the raw
    /// Groq body — the readable part fell off the end.
    #[test]
    fn rate_limits_become_words_instead_of_a_raw_http_body() {
        let raw = anyhow::anyhow!(
            r#"transcription failed (429 Too Many Requests): {{"error":{{"message":"Rate limit reached for model `whisper-large-v3`"}}}}"#
        );
        assert_eq!(
            user_message(&raw),
            "Rate limited — wait a moment and try again"
        );

        // Anything unrecognised must survive intact rather than be guessed at.
        let other = anyhow::anyhow!("no microphone found");
        assert_eq!(user_message(&other), "no microphone found");
    }

    /// Only testable since the Accessibility read moved out to the caller —
    /// before that, this function could not run without a focused app.
    #[test]
    fn app_rule_applies_only_to_the_app_it_matches() {
        let settings = Settings {
            post_processing_enabled: true,
            app_rules: vec![aloud_store::rules::AppRule {
                app_matches: "slack".into(),
                post_processing_enabled: Some(false),
                custom_prompt: String::new(),
            }],
            ..Default::default()
        };

        let matched = apply_app_rule(settings.clone(), Some("Slack"));
        assert!(!matched.post_processing_enabled, "the rule must apply");

        let other = apply_app_rule(settings.clone(), Some("Xcode"));
        assert!(other.post_processing_enabled, "a non-match must not apply");

        // No focused app at all — the read can legitimately return None.
        let none = apply_app_rule(settings, None);
        assert!(none.post_processing_enabled, "None must not apply a rule");
    }

    /// An app rule's prompt is a style hint, not a replacement. Replacing the
    /// base prompt made a "keep it casual" Slack rule silently discard every
    /// cleanup instruction (punctuation, filler removal, the EMPTY sentinel).
    #[test]
    fn app_rule_prompt_appends_to_the_base_instead_of_replacing_it() {
        let hint = "Keep it casual, no punctuation.";

        let on_default = prompt_with_hint("", hint);
        assert!(
            on_default.starts_with(aloud_llm::prompts::CLEANUP.trim_end()),
            "the built-in cleanup prompt must survive"
        );
        assert!(on_default.ends_with(hint));

        let on_custom = prompt_with_hint("MY CUSTOM PROMPT", hint);
        assert!(on_custom.starts_with("MY CUSTOM PROMPT"));
        assert!(on_custom.ends_with(hint));
    }

    /// The failure this guards: with a start delay, a tap shorter than the
    /// delay releases before the recorder exists. The stop path finds nothing
    /// to take and returns "not recording", and without invalidation the
    /// delayed start then opens the mic with nothing left to stop it — a
    /// capture that runs until the next press.
    #[test]
    fn a_stop_inside_the_delay_cancels_the_pending_start() {
        let mut session = Session::default();
        let generation = session.reserve_start();
        session.pending_start = true;
        assert!(session.is_active(), "a reserved start counts as recording");

        // The release lands before the delay elapses.
        session.invalidate_start();
        assert!(!session.is_active());
        assert!(
            !session.may_begin(generation),
            "the delayed start must not open the mic after a stop"
        );
    }

    /// A press arriving mid-delay must not queue a second capture, and must
    /// not invalidate the first — `start` returns early on an active session.
    #[test]
    fn a_second_press_inside_the_delay_leaves_the_first_start_intact() {
        let mut session = Session::default();
        let generation = session.reserve_start();
        session.pending_start = true;

        // `start` sees is_active() and returns without touching the session.
        assert!(session.is_active());
        assert!(
            session.may_begin(generation),
            "the original start must still be allowed to proceed"
        );
    }

    /// An undelayed start still works: same reserve, immediately honoured.
    #[test]
    fn an_undelayed_start_may_begin_at_once() {
        let mut session = Session::default();
        let generation = session.reserve_start();
        assert!(session.may_begin(generation));
    }

    /// With the key UI gone, a user whose cloud key does not resolve has no
    /// field to go fix it — erroring would leave dictation dead with no
    /// recovery. Cloud without a key must degrade to on-device instead.
    #[test]
    fn cloud_without_a_key_falls_back_to_local() {
        let mut settings = Settings {
            engine: Engine::Cloud,
            ..Default::default()
        };
        assert_eq!(effective_engine(&settings), Engine::Local);

        settings.groq_api_key = "   ".into();
        assert_eq!(
            effective_engine(&settings),
            Engine::Local,
            "whitespace is not a key"
        );

        settings.groq_api_key = "gsk_real".into();
        assert_eq!(
            effective_engine(&settings),
            Engine::Cloud,
            "a real key must still reach the cloud"
        );
    }
}
