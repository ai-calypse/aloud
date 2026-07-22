//! The dictation loop: hotkey → record → transcribe → clean up → paste.
//!
//! Orchestration only — every step lives in a crate that is testable without
//! launching the app.

use aloud_hotkey::model::Role;
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
/// Correlates every stage of one dictation. A counter, not a UUID: it sorts by
/// time for free, which holds only because the store seeds it from disk.
use aloud_store::timings::next_utterance_id;

/// 20 Hz is smooth to the eye without flooding the event channel.
const LEVEL_INTERVAL: Duration = Duration::from_millis(50);

/// How long muting waits so the start chime is still audible. Tink is roughly
/// 250ms; the margin covers `afplay`'s own spawn.
const START_CHIME_LEAD: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "dictation-phase")]
pub enum Phase {
    Idle,
    Recording,
    Transcribing,
    Done,
    /// Silence or filler only — a normal result, but its own phase rather than
    /// `Idle` so the overlay can say so instead of claiming "Pasted".
    NoSpeech,
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
    /// Taken alongside the recorder, so the two cannot outlive each other.
    preview: Option<aloud_transcribe::preview::Preview>,
    /// The last transcript, for "paste again".
    last_transcript: Option<String>,
    /// The in-flight read of the selected text. A handle rather than a value:
    /// reading it can take a clipboard round-trip, which must not happen on the
    /// hotkey thread.
    selection: Option<tauri::async_runtime::JoinHandle<Option<String>>>,
    started_at: Option<std::time::Instant>,
    /// A start that is reserved but still inside its delay, so `recorder` is
    /// None and yet the session is taken — it counts as recording everywhere,
    /// or a second press during the delay queues a second capture. The
    /// generation lives here rather than beside a flag so the two cannot drift.
    pending_start: Option<u64>,
    starts: u64,
    /// Which role owns the running session.
    ///
    /// Overlapping bindings (`fn` and `⌥fn`) activate the shorter one first, so
    /// without an owner the shorter role's release edge tore down the session
    /// the longer one was building. The owner lets a more-specific role
    /// supersede, and makes a superseded role's release a no-op.
    owner: Option<Role>,
}

impl Session {
    /// Claims the session for a start and returns the generation a delayed
    /// start must still see when it wakes.
    fn reserve_start(&mut self) -> u64 {
        self.starts = self.starts.wrapping_add(1);
        self.starts
    }

    /// Abandons any reserved start. Every stop path calls this, so a release
    /// arriving inside the delay wins over the start it is racing.
    fn invalidate_start(&mut self) {
        self.pending_start = None;
        self.owner = None;
        self.starts = self.starts.wrapping_add(1);
    }

    /// Whether a delayed start holding `generation` may still open the mic.
    fn may_begin(&self, generation: u64) -> bool {
        self.pending_start == Some(generation)
    }

    /// A capture is running, or reserved and about to.
    fn is_active(&self) -> bool {
        self.recorder.is_some() || self.pending_start.is_some()
    }
}

/// Whether a role's capture gets the live transcript in the pill. Dictation
/// only: for every other role the running words contradict what the panel is
/// there to say, and the preview behind them is a core spent on nothing.
fn shows_transcript(role: Option<Role>) -> bool {
    matches!(role, Some(Role::Hold | Role::Toggle))
}

/// What a stop takes out of the session.
struct Captured {
    recorder: Option<aloud_audio::Recorder>,
    started_at: Option<std::time::Instant>,
    /// Never read — held only so that dropping `Captured` stops the preview.
    _preview: Option<aloud_transcribe::preview::Preview>,
}

pub struct Pipeline {
    app: AppHandle,
    session: Mutex<Session>,
    settings: Arc<Mutex<Settings>>,
    history: Mutex<History>,
    stats: Mutex<Stats>,
    /// The last phase emitted, so the tray can show it without the frontend
    /// being awake.
    phase: Mutex<Phase>,
    /// Loaded lazily: the model costs ~200ms and memory.
    local: Mutex<Option<Arc<dyn Transcriber>>>,
    /// Read by the hotkey thread to decide whether Escape is ours to swallow.
    busy: Arc<std::sync::atomic::AtomicBool>,
    /// Racing this against the work drops the transcribe/LLM future, which
    /// aborts the HTTP request — dropping the recorder alone left it running
    /// and the pill up for as long as 80s.
    cancelled: tokio::sync::Notify,
    /// `notify_waiters` only wakes already-registered waiters, so an Escape
    /// between the phase change and the `select!` would be lost and the
    /// transcript would paste anyway. The work checks this before using its
    /// result.
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
            // Loaded, not fresh: a default Stats here zeroes the user's
            // lifetime words, sessions and streak on every launch.
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

    /// Publishes the phase and drives the overlay's visibility with it, so the
    /// pill can never be left on screen after a session ends.
    fn emit(&self, phase: Phase) {
        if let Ok(mut current) = self.phase.lock() {
            *current = phase;
        }
        // Escape is only ours to swallow while something is actually running.
        self.busy.store(
            matches!(phase, Phase::Recording | Phase::Transcribing),
            std::sync::atomic::Ordering::Relaxed,
        );
        crate::tray::refresh(&self.app);

        if let Err(e) = phase.emit(&self.app) {
            tracing::warn!(%e, "could not emit phase");
        }

        let chime = match phase {
            Phase::Recording => Some(crate::sound::Chime::Start),
            Phase::Transcribing => Some(crate::sound::Chime::Stop),
            Phase::Error => Some(crate::sound::Chime::Error),
            // Silence is not a failure, so no error chime.
            Phase::Done | Phase::Idle | Phase::NoSpeech => None,
        };
        if let Some(chime) = chime {
            let settings = self.settings();
            crate::sound::play(chime, settings.play_sounds, settings.sound_volume);
        }

        match phase {
            Phase::Recording => crate::overlay::show(&self.app, phase),
            // The transcript panel stays up through transcription: the last
            // preview pass is the only place the end of the sentence is
            // readable.
            Phase::Transcribing => crate::overlay::show(&self.app, phase),
            // Re-shown rather than left visible: an error in the wings style
            // has to move to the pill's anchor, and only `show` repositions.
            Phase::Done | Phase::NoSpeech | Phase::Error => {
                crate::overlay::show(&self.app, phase);
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

    /// Reports a failure to the user, not just to the log.
    fn emit_error(&self, error: &anyhow::Error) {
        crate::overlay::send_error(&self.app, &user_message(error));
        self.emit(Phase::Error);
    }

    fn settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn phase(&self) -> Phase {
        self.phase.lock().map(|p| *p).unwrap_or(Phase::Idle)
    }

    pub fn last_transcript(&self) -> Option<String> {
        self.session
            .lock()
            .ok()
            .and_then(|s| s.last_transcript.clone())
    }

    /// Whether a capture session is in flight, for the tap-to-toggle hotkey. A
    /// reserved-but-not-yet-started capture counts, or the toggle would queue a
    /// second one during the start delay.
    pub fn is_recording(&self) -> bool {
        self.session.lock().map(|s| s.is_active()).unwrap_or(false)
    }

    /// Whether `role` still owns the running session, so its release should
    /// dispose of the capture. A superseded role's release must be dropped, or
    /// it stops the session the winning role is still running.
    pub fn owns(&self, role: Role) -> bool {
        self.session
            .lock()
            .map(|s| s.owner == Some(role))
            .unwrap_or(false)
    }

    /// Takes the recorder for a stop path, and invalidates any pending start —
    /// otherwise a stop inside the start delay leaves the delayed start to open
    /// a capture that runs until the next press.
    fn take_recorder(&self) -> Captured {
        let mut session = self.session.lock().unwrap();
        session.invalidate_start();
        let taken = Captured {
            recorder: session.recorder.take(),
            started_at: session.started_at.take(),
            _preview: session.preview.take(),
        };
        drop(session);

        // Not gated on the setting: turning muting off mid-dictation leaves the
        // volume down, and a restore with nothing saved is a no-op. Queued
        // because Escape reaches `cancel` on the event-tap thread, where the
        // `osascript` this ends in would stall every keystroke on the machine.
        aloud_volume::restore_soon();
        taken
    }

    /// Starts capturing. Idempotent: a second call while recording is a no-op.
    ///
    /// Must return immediately — this runs on the CGEventTap callback thread,
    /// where sleeping stalls every keystroke on the machine and can trip
    /// `kCGEventTapDisabledByTimeout`, which kills the listener silently. So a
    /// non-zero `shortcut_start_delay_ms` reserves the slot and starts capture
    /// from the runtime once the delay elapses.
    pub fn start(self: &Arc<Self>, role: Role) -> Result<()> {
        let settings = self.settings();
        let delay = settings.shortcut_start_delay_ms;
        let mut session = self.session.lock().unwrap();
        if session.is_active() {
            // A more-specific role takes the session over (`⌥fn` after `fn`);
            // a less- or equally-specific one leaves the owner alone, which
            // keeps the shorter binding from stealing it back.
            let specificity =
                |r: Option<Role>| r.map_or(0, |r| settings.hotkeys.binding(r).specificity());
            if specificity(Some(role)) > specificity(session.owner) {
                session.owner = Some(role);
                // What began as a dictation is now an ask, so the preview goes
                // back with it — and the pill is told, or it keeps showing the
                // words the superseded dictation had reached.
                if !shows_transcript(Some(role)) {
                    session.preview = None;
                    crate::overlay::send_partial(&self.app, "");
                }
            }
            return Ok(());
        }
        let generation = session.reserve_start();
        session.owner = Some(role);

        if delay == 0 {
            self.begin_capture(session)?;
            self.emit(Phase::Recording);
            return Ok(());
        }

        session.pending_start = Some(generation);
        drop(session);

        // Now rather than after the delay: silence until capture begins reads
        // as a dropped hotkey. The delayed start does not emit again — a second
        // Recording would replay the start chime and rebuild the tray menu.
        self.emit(Phase::Recording);

        let pipeline = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            let session = pipeline.session.lock().unwrap();
            // Released, cancelled, or restarted while we slept.
            if !session.may_begin(generation) {
                return;
            }
            if let Err(e) = pipeline.begin_capture(session) {
                tracing::error!(%e, "could not start recording after the delay");
                pipeline.emit_error(&e);
            }
        });
        Ok(())
    }

    /// Opens the microphone and marks the session live. Takes the session guard
    /// so the caller's generation check and this cannot be raced apart. Does
    /// not emit — the delayed path has already shown the pill.
    fn begin_capture(
        self: &Arc<Self>,
        mut session: std::sync::MutexGuard<'_, Session>,
    ) -> Result<()> {
        let settings = self.settings();

        // Armed *before* the microphone opens: the tap is a constructor
        // argument, so earlier audio only the WAV path ever sees. Its
        // transcriber is built by the worker, not here — this is the event-tap
        // thread, where a whisper model load stalls every keystroke.
        let preview =
            (settings.streaming_transcription && shows_transcript(session.owner)).then(|| {
                let pipeline = self.clone();
                let for_build = settings.clone();
                let app = self.app.clone();
                aloud_transcribe::preview::Preview::spawn(
                    // Tauri's handle: this thread is usually the OS key-event
                    // tap and has no runtime of its own.
                    tauri::async_runtime::handle().inner(),
                    async move { pipeline.preview_transcriber(&for_build).await },
                    settings.vocabulary.clone(),
                    Box::new(move |partial| crate::overlay::send_partial(&app, partial)),
                )
            });
        let tap = preview.as_ref().map(|p| p.tap());
        let recorder =
            match aloud_audio::Recorder::start(settings.selected_microphone.as_deref(), tap) {
                Ok(recorder) => recorder,
                Err(e) => {
                    // Leaving this set would make every later start a no-op.
                    session.pending_start = None;
                    return Err(e).context("starting recorder");
                }
            };

        // The handle lives only as long as the recorder, so this task ends by
        // itself — no stop flag to keep in sync with five stop paths.
        let level = recorder.level_handle();
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            while Arc::strong_count(&level) > 1 {
                let value = f32::from_bits(level.load(std::sync::atomic::Ordering::Relaxed));
                crate::overlay::send_level(&app, value);
                tokio::time::sleep(LEVEL_INTERVAL).await;
            }
        });

        // Queued, not run here: with no start delay this runs on the event-tap
        // thread, where `osascript` stalls every keystroke. Queuing also keeps
        // it ordered against the restore on the stop path. The lead keeps the
        // start chime audible — it plays through the output about to be muted,
        // and it is the only cue telling the user when to speak.
        if settings.mute_system_audio {
            let lead = if settings.play_sounds {
                START_CHIME_LEAD
            } else {
                Duration::ZERO
            };
            aloud_volume::mute_soon(lead);
        }

        session.pending_start = None;
        session.recorder = Some(recorder);
        session.preview = preview;
        // From when audio starts, not the key press, so the start delay is not
        // billed to the user as speaking time.
        session.started_at = Some(std::time::Instant::now());
        Ok(())
    }

    /// Stops capturing, transcribes, cleans up and pastes.
    ///
    /// The span opens on the release edge so its duration is the latency the
    /// user feels. The id is minted by the caller rather than inside `fields(…)`
    /// because a field expression's value cannot be read back off the span, and
    /// the history row has to be stamped with it.
    pub async fn stop_and_paste(&self) -> Result<()> {
        self.dictate(next_utterance_id()).await
    }

    #[tracing::instrument(
        name = "utterance",
        skip_all,
        fields(
            utterance_id = utterance_id,
            mode = "dictation",
            spoken_ms = tracing::field::Empty,
            outcome = tracing::field::Empty,
            ok = tracing::field::Empty,
        )
    )]
    async fn dictate(&self, utterance_id: i64) -> Result<()> {
        let captured = self.take_recorder();
        if captured.recorder.is_none() {
            return Ok(()); // not recording
        }
        let started_at = captured.started_at;

        let span = tracing::Span::current();
        if let Some(start) = started_at {
            span.record("spoken_ms", start.elapsed().as_millis() as u64);
        }

        let generation = self.generation();
        self.emit(Phase::Transcribing);
        let result = tokio::select! {
            result = self.transcribe_and_paste(captured, started_at, generation, utterance_id) => result,
            // Dropping the work future is what aborts the in-flight request.
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
            Ok(Outcome::Pasted) => self.emit(Phase::Done),
            Ok(Outcome::NoSpeech) => self.emit(Phase::NoSpeech),
            // The Ask panel is the feedback; the pill would just cover it. A
            // cancel is the user's own doing and needs no report.
            Ok(Outcome::Asked) | Ok(Outcome::Cancelled) => self.emit(Phase::Idle),
            Err(e) => {
                tracing::error!(%e, "dictation failed");
                self.emit_error(e);
            }
        }
        result.map(|_| ())
    }

    /// The shared shape of a stop: take the recorder, show Transcribing, run
    /// the mode's work, then settle the pill on `done` or the error.
    ///
    /// The public `stop_and_*` keep their own `#[tracing::instrument]` — the
    /// `mode` field differs per mode and must not be shared.
    async fn run_stage<F, Fut>(&self, done: Phase, work: F) -> Result<()>
    where
        F: FnOnce(Captured) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let captured = self.take_recorder();
        if captured.recorder.is_none() {
            return Ok(());
        }

        self.emit(Phase::Transcribing);
        let result = work(captured).await;
        if result.is_err() {
            tracing::Span::current().record("ok", false);
        }
        match &result {
            Ok(()) => self.emit(done),
            Err(e) => self.emit_error(e),
        }
        result
    }

    /// Finalizes the recording, transcribes it, and returns the WAV only when
    /// the caller is going to write a history row pointing at it.
    ///
    /// Ask, agent and rewrite pass `false`: they keep no history row, so a kept
    /// recording would be an orphan — invisible to the size budget, unreachable
    /// from the Run Log, and a permanent copy of a question never chosen to be
    /// stored. Their audio goes to the OS temp dir and is unlinked here.
    async fn capture(
        &self,
        captured: Captured,
        persist: bool,
    ) -> Result<(String, Settings, Option<PathBuf>)> {
        // The preview drops here with the rest of `Captured`: the key is up, so
        // a pass still running is CPU the real transcription wants.
        let Captured { recorder, .. } = captured;
        let Some(recorder) = recorder else {
            bail!("no recording to transcribe");
        };
        // A media directory we cannot create must not take dictation down with
        // it: the recording is a nicety, the transcript is the product.
        let keep = match persist.then(aloud_notes::media_wav_path).transpose() {
            Ok(path) => path,
            Err(e) => {
                tracing::warn!(%e, "no media directory; this recording will not be kept");
                None
            }
        };
        // Armed *before* the file can exist, not after transcription succeeds:
        // every `?` below otherwise returns `Err` leaving a WAV in the media
        // directory that nothing reaps, no history row sees, and "Clear all
        // history" cannot find.
        let mut guard = KeptWav(keep);
        let wav = guard.0.clone().unwrap_or_else(temp_wav_path);
        // Draining the buffer and writing the WAV is invisible latency between
        // the key release and the first byte the transcriber sees.
        {
            let span = tracing::info_span!("finalize_audio", wav_bytes = tracing::field::Empty);
            let _e = span.enter();
            recorder.stop_to_wav(&wav)?;
            if let Ok(meta) = std::fs::metadata(&wav) {
                span.record("wav_bytes", meta.len());
            }
        }

        let settings = self.settings();

        // The live preview is deliberately *not* consulted: it re-reads a
        // rolling window with the small local model, which is the wrong trade
        // for text about to land in someone's document. That gets one pass over
        // the whole recording on the engine the user chose.

        let text = async {
            // Recorded up front so a *failed* transcription still names an
            // engine — the row where it matters most.
            let span = tracing::Span::current();
            span.record("engine", tracing::field::debug(effective_engine(&settings)));

            let (text, engine) = self.transcribe_with_local_fallback(&settings, &wav).await?;
            // Overwritten with the engine that actually *ran*, which differs
            // from the configured one after a silent fallback.
            span.record("engine", tracing::field::debug(engine));
            span.record("chars", text.len());
            anyhow::Ok(text)
        }
        .instrument(tracing::info_span!(
            "transcribe",
            engine = tracing::field::Empty,
            chars = tracing::field::Empty,
        ))
        .await?;
        let keep = guard.keep();
        // The non-persisted modes wrote to the temp dir, which the guard does
        // not own.
        if keep.is_none() {
            let _ = std::fs::remove_file(&wav);
        }
        Ok((text, settings, keep))
    }

    async fn transcribe_and_paste(
        &self,
        captured: Captured,
        started_at: Option<std::time::Instant>,
        generation: u64,
        utterance_id: i64,
    ) -> Result<Outcome> {
        // Now, not after: an Accessibility call blocks on the *target* app's
        // run loop (a busy Chrome makes it unbounded) so it overlaps the
        // transcribe — and post-paste the focused app is whatever the paste
        // chord left in front.
        let app_name = tauri::async_runtime::spawn_blocking(aloud_selection::frontmost_app);

        let (raw, settings, wav) = self.capture(captured, true).await?;
        let mut wav = KeptWav(wav);

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

        // Applied to a working copy: history still records against the real
        // config, only cleanup behaviour changes.
        let focused = app_name.await.ok().flatten();
        let app_name = focused.as_ref().map(|f| f.name.clone());
        let effective = apply_app_rule(
            settings.clone(),
            app_name.as_deref(),
            focused.as_ref().and_then(|f| f.bundle_id.as_deref()),
        );

        let final_text = self.post_process(&raw, &effective).await;
        if final_text.trim().is_empty() {
            return Ok(Outcome::NoSpeech);
        }

        // Last point before the text lands in someone else's app: a cancel that
        // raced the `select!` registration is only honoured here, and pasting
        // after Escape is the one failure the user cannot undo.
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
            utterance_id,
            wav.keep(),
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
                    preserve: &settings.preserve_phrases,
                    output_language: &settings.output_language,
                    instruction_guard: settings.instruction_guard_enabled,
                };
                match aloud_llm::cleanup::clean(options, raw).await {
                    Ok(text) => text,
                    Err(e) => {
                        tracing::warn!(%e, "post-processing failed; pasting raw transcript");
                        raw.to_string()
                    }
                }
            };

        // After cleanup, so the model's spacing/casing fixes land before a
        // trigger is matched.
        if settings.macros.is_empty() {
            cleaned
        } else {
            aloud_store::macros::expand(&cleaned, &settings.macros)
        }
    }

    /// Loads the local model ahead of the first dictation, so the first
    /// utterance does not pay a 150MB read plus the Metal pipeline build.
    /// Silent: the real attempt inside `capture` surfaces a failure properly.
    pub fn warm_up(self: &Arc<Self>) {
        let pipeline = self.clone();
        tauri::async_runtime::spawn(async move {
            let settings = pipeline.settings();
            if effective_engine(&settings) != Engine::Local {
                return;
            }
            let started = std::time::Instant::now();
            match pipeline.transcriber(&settings).await {
                Ok(_) => tracing::info!(ms = started.elapsed().as_millis(), "warmed local model"),
                Err(e) => tracing::warn!(%e, "could not warm the local model"),
            }
        });
    }

    /// Downloads the on-device model in the background so the *next* cloud
    /// failure has something to fall back to. Never awaited by the dictation
    /// that triggered it — the point is to keep a 150MB fetch out of it.
    fn fetch_local_model_soon(&self) {
        tauri::async_runtime::spawn(async move {
            let Ok(dir) = aloud_transcribe::local::model_dir() else {
                return;
            };
            tracing::info!("fetching the on-device model so the next fallback can use it");
            // No progress to the overlay: no dictation is waiting on this.
            match aloud_transcribe::download::ensure_model(
                &dir,
                aloud_transcribe::local::DEFAULT_MODEL,
                |_| {},
            )
            .await
            {
                Ok(_) => tracing::info!("on-device model ready for the next fallback"),
                Err(e) => tracing::warn!(%e, "could not fetch the on-device model"),
            }
        });
    }

    /// Transcribes, retrying on-device once if the cloud call failed for a
    /// reason on-device would not, and reports which engine produced the text.
    /// Once, not in a loop: the user is holding a pill open, and the on-device
    /// engine either works or is broken in a way a retry will not fix.
    async fn transcribe_with_local_fallback(
        &self,
        settings: &Settings,
        wav: &std::path::Path,
    ) -> Result<(String, Engine)> {
        let engine = effective_engine(settings);
        let error = match self
            .transcriber(settings)
            .await?
            .transcribe(wav, &settings.vocabulary)
            .await
        {
            Ok(text) => return Ok((text, engine)),
            Err(e) if engine == Engine::Cloud && aloud_transcribe::is_recoverable_locally(&e) => e,
            Err(e) => return Err(e),
        };

        // Only when the model is already on disk: `ensure_model` would put a
        // 150MB download inside the dictation the user is waiting on — over a
        // network that, on the transport-error path, is what just broke.
        // Fetching in the background covers the *next* failure instead.
        if !aloud_transcribe::local::model_is_present() {
            self.fetch_local_model_soon();
            return Err(error
                .context("cloud transcription failed and no on-device model is downloaded yet"));
        }

        tracing::warn!(%error, "cloud transcription failed; retrying on-device");
        let mut local = settings.clone();
        local.engine = Engine::Local;
        let text = self
            .transcriber(&local)
            .await?
            .transcribe(wav, &settings.vocabulary)
            .await
            // Without the cloud error the user is told the local model failed
            // and never learns why it was reached for.
            .with_context(|| format!("after the cloud engine failed: {error:#}"))?;
        Ok((text, Engine::Local))
    }

    /// The configured engine. The local model is cached (loading costs ~200ms
    /// and memory); the cloud client is cheap and is not. Keyed on the
    /// *effective* engine so a cloud-to-local fallback caches too.
    async fn transcriber(&self, settings: &Settings) -> Result<Arc<dyn Transcriber>> {
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

    /// The model the live preview runs on: always the local one, whatever
    /// engine the pasted transcript uses. The preview re-transcribes its window
    /// several times a second, and those uploads would take the real
    /// transcription down with them through the rate limiter.
    ///
    /// Never downloads: no model on disk means no preview, and the dictation is
    /// unaffected.
    async fn preview_transcriber(&self, settings: &Settings) -> Result<Arc<dyn Transcriber>> {
        if let Some(local) = self.local.lock().unwrap().clone() {
            return Ok(local);
        }
        let model =
            aloud_transcribe::local::model_dir()?.join(aloud_transcribe::local::DEFAULT_MODEL);
        if !model.exists() {
            bail!("no local model downloaded; skipping the live preview");
        }
        let language = Some(settings.language.as_str()).filter(|l| !l.is_empty());
        let transcriber: Arc<dyn Transcriber> = Arc::new(LocalTranscriber::new(&model, language)?);
        *self.local.lock().unwrap() = Some(transcriber.clone());
        Ok(transcriber)
    }

    /// Records the dictation for history and stats. Never fails the paste:
    /// the text is already in the user's document.
    // Same reasoning as `History::insert`: these are the columns of one row,
    // and a params struct would be an `Entry` the sole caller builds only to
    // have it taken apart again.
    #[allow(clippy::too_many_arguments)]
    fn record(
        &self,
        raw: &str,
        final_text: &str,
        started_at: Option<std::time::Instant>,
        settings: &Settings,
        app_name: Option<&str>,
        utterance_id: i64,
        audio_path: Option<PathBuf>,
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
                Some(utterance_id),
                // Lossy paths are dropped, not mangled: one that does not
                // round-trip through UTF-8 cannot reach the asset protocol
                // either, so an approximation is just a play button that 404s.
                audio_path.as_deref().and_then(|p| p.to_str()),
                settings.retention(),
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

    /// Notes the selection for a plain dictation, if the OS will say what it is
    /// without being asked twice.
    ///
    /// The quiet read: this runs on every utterance, and the clipboard fallback
    /// would synthesise a Cmd+C into the user's app each time. Where
    /// Accessibility cannot answer, we simply dictate.
    pub fn note_selection_if_any(&self) {
        // Off the hotkey thread: an Accessibility read waits on the target app.
        let handle = tauri::async_runtime::spawn_blocking(aloud_selection::selected_text_quietly);
        if let Ok(mut session) = self.session.lock() {
            session.selection = Some(handle);
        }
    }

    /// Starts reading the selection, without waiting for it. The hotkey thread
    /// must never block — a stalled event tap stops delivering keys
    /// system-wide — and this read can fall back to a Cmd+C round-trip, so it
    /// goes to a blocking worker that `stop_and_dictate` awaits.
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
    /// One stop path for both hotkeys: whatever the read started on key-down
    /// found decides which — something selected makes the utterance an
    /// instruction for changing it, nothing selected makes it the text itself.
    ///
    /// Not instrumented: both branches open their own `utterance` span, so a
    /// span here would nest inside itself and report every dictation twice.
    pub async fn stop_and_dictate(&self) -> Result<()> {
        let reading = self.session.lock().unwrap().selection.take();
        let selection = match reading {
            Some(handle) => handle.await.ok().flatten(),
            None => None,
        };

        match selection {
            Some(selection) => self.finish_rewrite(selection).await,
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

    async fn transcribe_and_rewrite(&self, captured: Captured, selection: &str) -> Result<()> {
        let (command, settings, _) = self.capture(captured, false).await?;

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
        // The panel is the feedback; the pill only stays for an error.
        self.run_stage(Phase::Idle, |r| self.transcribe_and_ask(r))
            .await
    }

    async fn transcribe_and_ask(&self, captured: Captured) -> Result<()> {
        let (question, settings, _) = self.capture(captured, false).await?;

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
        // The panel is the feedback; the pill only stays for an error.
        self.run_stage(Phase::Idle, |r| self.transcribe_and_agent(r))
            .await
    }

    async fn transcribe_and_agent(&self, captured: Captured) -> Result<()> {
        let (command, settings, _) = self.capture(captured, false).await?;

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
        // Through take_recorder so a start still inside its delay is
        // invalidated too, or the capture would begin anyway.
        let _ = self.take_recorder();
        // Wakes the select! in the stop_* paths; a no-op during plain
        // recording, which the emit below ends instead.
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
        self.history_db()?.recent(limit)
    }

    /// The history store, for the Run Log's row-level edits. Hands out the
    /// guard rather than forwarding three one-line statements. Returns an error
    /// rather than unwrapping: the tray builds its History submenu from this on
    /// the main thread, where a poisoned lock would panic the menu build.
    pub fn history_db(&self) -> Result<std::sync::MutexGuard<'_, History>> {
        self.history
            .lock()
            .map_err(|_| anyhow::anyhow!("history lock poisoned"))
    }

    /// Re-transcribes a stored recording and rewrites the run with the result,
    /// through the live pipeline's own `transcriber` and `post_process` so the
    /// retry cannot clean up differently from the real dictation.
    ///
    /// `engine` overrides the configured one for this run only. Retrying local
    /// after a cloud run pays a model load — or a 150MB download on a machine
    /// that has never held one. It looks like a hang and is not one.
    pub async fn retry_transcription(&self, id: i64, engine: Option<Engine>) -> Result<String> {
        let entry = self
            .history_db()?
            .get(id)?
            .context("that run is no longer in the history")?;
        let wav = entry
            .audio_path
            .context("no recording was kept for this run, so there is nothing to re-transcribe")?;
        let wav = PathBuf::from(wav);
        // The row can outlive the file: the budget nulls the column when it
        // evicts, but a hand-deleted file leaves the path behind.
        if !wav.exists() {
            bail!("the recording for this run is no longer on disk");
        }

        let mut settings = self.settings();
        if let Some(engine) = engine {
            settings.engine = engine;
        }
        let raw = self
            .transcriber(&settings)
            .await?
            .transcribe(&wav, &settings.vocabulary)
            .await?;
        if raw.trim().is_empty() {
            bail!("the recording transcribed to nothing — it may be silence");
        }

        let final_text = self.post_process(&raw, &settings).await;
        // Both halves: leaving the old "Heard" beside the new "Pasted" would
        // describe two different runs.
        self.history_db()?
            .update_transcripts(id, Some(&raw), &final_text)?;
        Ok(final_text)
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

/// Builds the configured transcription engine. `app` is only used to report
/// model-download progress; the transcriber itself has no Tauri dependency.
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

/// Overrides `settings` with the rule for `app`, if one matches. Takes the name
/// rather than reading it: the Accessibility call blocks on the target app, so
/// the caller does it off-thread and reuses the name for the history record.
fn apply_app_rule(mut settings: Settings, app: Option<&str>, bundle_id: Option<&str>) -> Settings {
    if settings.app_rules.is_empty() {
        return settings;
    }
    let Some(rule) = aloud_store::rules::matching(&settings.app_rules, app, bundle_id) else {
        return settings;
    };
    tracing::info!(app = ?app, matcher = %rule.app_matches, "applying app rule");

    if let Some(enabled) = rule.post_processing_enabled {
        settings.post_processing_enabled = enabled;
    }
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

/// A failure in words the user can act on. The pill is one truncated line, so a
/// raw 429 body is cut off long before the part that matters. Anything
/// unrecognised falls through unchanged rather than being guessed at.
fn user_message(error: &anyhow::Error) -> String {
    let full = format!("{error:#}");
    if full.contains("429") || full.to_lowercase().contains("rate limit") {
        // The wait is in headers that are gone by here, and the LLM path
        // reports its own cooldown.
        return "Rate limited — wait a moment and try again".to_string();
    }
    if full.contains("401") || full.contains("invalid_api_key") {
        // Not a fallback case: the key is present, just rejected, so
        // `effective_engine` still picks Cloud and the next call fails too.
        return "Credential rejected — switch to on-device in Settings".to_string();
    }
    if full.contains("timed out") || full.contains("operation timed out") {
        return "Timed out — check your connection".to_string();
    }
    full
}

/// Owns a kept recording until a history row takes it, and deletes it if none
/// does. Every early return between transcription and the insert would
/// otherwise leave a WAV the size budget cannot see, because it only looks at
/// recordings the history knows about. A guard rather than a delete per branch
/// so the next early return added here is covered by default.
struct KeptWav(Option<PathBuf>);

impl KeptWav {
    /// Hands the recording to the caller and disarms the delete.
    fn keep(&mut self) -> Option<PathBuf> {
        self.0.take()
    }
}

impl Drop for KeptWav {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
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
    /// body — the readable part falls off the end.
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

    #[test]
    fn app_rule_applies_only_to_the_app_it_matches() {
        let settings = Settings {
            post_processing_enabled: true,
            app_rules: vec![aloud_store::rules::AppRule {
                app_matches: "slack".into(),
                bundle_id: None,
                post_processing_enabled: Some(false),
                custom_prompt: String::new(),
            }],
            ..Default::default()
        };

        let matched = apply_app_rule(settings.clone(), Some("Slack"), None);
        assert!(!matched.post_processing_enabled, "the rule must apply");

        let other = apply_app_rule(settings.clone(), Some("Xcode"), None);
        assert!(other.post_processing_enabled, "a non-match must not apply");

        // No focused app at all — the read can legitimately return None.
        let none = apply_app_rule(settings, None, None);
        assert!(none.post_processing_enabled, "None must not apply a rule");
    }

    /// An app rule's prompt is a style hint, not a replacement: replacing the
    /// base made a "keep it casual" rule silently discard punctuation repair,
    /// filler removal and the EMPTY sentinel too.
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

    /// The failure this guards: a tap shorter than the start delay releases
    /// before the recorder exists, and without invalidation the delayed start
    /// opens a mic with nothing left to stop it.
    #[test]
    fn a_stop_inside_the_delay_cancels_the_pending_start() {
        let mut session = Session::default();
        let generation = session.reserve_start();
        session.pending_start = Some(generation);
        assert!(session.is_active(), "a reserved start counts as recording");
        assert!(session.may_begin(generation));

        // The release lands before the delay elapses.
        session.invalidate_start();
        assert!(!session.is_active());
        assert!(
            !session.may_begin(generation),
            "the delayed start must not open the mic after a stop"
        );
    }

    /// A stop clears the owner, so a superseded role's late release edge finds
    /// none and is dropped rather than stopping a fresh capture.
    #[test]
    fn a_stop_clears_the_session_owner() {
        let mut session = Session::default();
        session.reserve_start();
        session.owner = Some(Role::Hold);
        assert_eq!(session.owner, Some(Role::Hold));

        session.invalidate_start();
        assert_eq!(
            session.owner, None,
            "a stopped session must own no role, or a stale release stops the next one"
        );
    }

    /// The panel is dictation's alone. Every role is named here so adding one
    /// is a decision, not a default.
    #[test]
    fn only_dictation_gets_the_transcript_panel() {
        for role in Role::ALL {
            let expected = matches!(role, Role::Hold | Role::Toggle);
            assert_eq!(
                shows_transcript(Some(role)),
                expected,
                "{role:?} disagrees about the live-transcript panel"
            );
        }
        assert!(!shows_transcript(None));
    }

    /// Press, release, press again — all inside the delay. Both sleepers wake;
    /// only the second may proceed, since the first holds a retired generation.
    #[test]
    fn a_restart_inside_the_delay_retires_the_first_start_only() {
        let mut session = Session::default();
        let first = session.reserve_start();
        session.pending_start = Some(first);

        session.invalidate_start(); // release
        let second = session.reserve_start(); // pressed again
        session.pending_start = Some(second);

        assert_ne!(first, second, "each start needs its own generation");
        assert!(!session.may_begin(first), "the retired start must lose");
        assert!(session.may_begin(second), "the live start must win");
    }

    /// With no key UI, erroring would leave dictation dead with no recovery —
    /// cloud without a key must degrade to on-device instead.
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
