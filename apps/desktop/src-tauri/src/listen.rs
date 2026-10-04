//! Always-listening mode: the mic stays open, the menu-bar wings say so, and
//! every spoken request runs as an agent task, queued behind the one running —
//! "open Notes", then "add eggs to my shopping note".
//!
//! Turning the mode on is the activation; there is no per-request trigger.
//! Room speech can therefore start a run, which is bounded the way every run
//! is: each connected-account write and each risky click waits for a yes.
//!
//! Hearing is two jobs. Deepgram's Flux, when a key is set, decides where a
//! request ends (it understands an unfinished sentence, where a pause detector
//! cuts it in two) and shows the words live under the wings. The request itself
//! is then re-transcribed from its audio by the dictation transcriber, because
//! Flux's streaming text misheard short commands — "ket means" for "cat memes".
//! Without a Deepgram key a pause detector cuts the turns instead, so a bundled
//! build without a `.env` still works.

use crate::pipeline::{Phase, Pipeline};
use aloud_store::Settings;
use aloud_transcribe::deepgram::{self, Heard};
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::mpsc;

/// Speech RMS a frame must reach in the pause-detector path. See `Segmenter`.
const SPEECH_RMS: f32 = 0.02;
/// Wait before reopening a dropped Deepgram stream.
const RECONNECT: Duration = Duration::from_secs(2);
/// Audio kept for the turn being said: what the transcriber gets when Flux
/// says it ended. Far longer than anyone speaks a command; the cap is for a
/// room that never goes quiet.
const MAX_TURN_BYTES: usize = aloud_audio::REALTIME_RATE as usize * 2 * 30;

/// What every part of a session shares. Holds no request sender, or the runner
/// would keep its own channel open and never end.
struct Shared {
    app: AppHandle,
    settings: Settings,
    pipeline: Arc<Pipeline>,
    /// Bumped by a cancel or by stopping: the running request stops between
    /// tools and the queued ones, stamped with an older value, are skipped.
    generation: AtomicU64,
    /// Requests being transcribed, waiting or running. The wings show work
    /// until it is back to zero.
    pending: AtomicUsize,
}

type Requests = mpsc::UnboundedSender<(u64, String)>;
/// A finished turn: Flux's text (None from the pause detector) and its audio.
type Turns = mpsc::UnboundedSender<(Option<String>, Vec<u8>)>;

impl Shared {
    fn busy(&self) -> bool {
        self.pending.load(Ordering::SeqCst) > 0
    }

    fn dictating(&self) -> bool {
        self.pipeline.busy_flag().load(Ordering::Relaxed)
    }

    /// Shows `phase` in the overlay, unless a dictation or an open panel owns
    /// it: the user is mid-sentence, or reading an answer.
    fn show(&self, phase: Phase) {
        if !self.dictating() && !crate::overlay::panel_open() {
            crate::overlay::show(&self.app, phase);
        }
    }

    fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.pending.store(0, Ordering::SeqCst);
        self.show(Phase::Listening);
    }
}

/// A running session. Dropping it closes the mic, stops the running request,
/// skips the queue and takes the wings down.
pub struct Listener {
    shared: Arc<Shared>,
    _mic: aloud_audio::Recorder,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        crate::overlay::LISTENING.store(false, Ordering::SeqCst);
        crate::overlay::send_partial(&self.shared.app, "");
        crate::overlay::rest(&self.shared.app);
    }
}

impl Listener {
    pub fn start(app: AppHandle, settings: Settings, pipeline: Arc<Pipeline>) -> Result<Self> {
        let (requests, queue) = mpsc::unbounded_channel();
        let (turns, finished) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            app,
            settings,
            pipeline,
            generation: AtomicU64::new(0),
            pending: AtomicUsize::new(0),
        });
        let mic = open_mic(&shared, turns).context("opening the always-listening mic")?;
        tauri::async_runtime::spawn(run_requests(shared.clone(), queue));
        tauri::async_runtime::spawn(take_turns(shared.clone(), requests, finished));
        crate::overlay::LISTENING.store(true, Ordering::SeqCst);
        shared.show(Phase::Listening);
        Ok(Self { shared, _mic: mic })
    }
}

/// Turns finished turns into requests, one at a time so they keep the order
/// they were said in.
async fn take_turns(
    shared: Arc<Shared>,
    requests: Requests,
    mut finished: mpsc::UnboundedReceiver<(Option<String>, Vec<u8>)>,
) {
    while let Some((heard, audio)) = finished.recv().await {
        crate::overlay::send_partial(&shared.app, "");
        // A cough or a door is a turn with no words; Flux says so for free.
        if heard.as_deref().is_some_and(|h| h.trim().is_empty()) {
            continue;
        }
        if let Some(said) = heard.as_deref().and_then(control) {
            act(&shared, said);
            continue;
        }
        shared.pending.fetch_add(1, Ordering::SeqCst);
        shared.show(Phase::Transcribing);
        let text = match shared.pipeline.transcribe_pcm16(&audio).await {
            Ok(text) if !text.trim().is_empty() => text,
            // The transcriber dropped it (silence, a hallucination) or failed:
            // Flux's own words are the next best thing, if it heard any.
            Ok(_) => heard.unwrap_or_default(),
            Err(e) => {
                tracing::warn!(%e, "always-listening transcription failed; using the live text");
                heard.unwrap_or_default()
            }
        };
        let text = text.trim();
        if text.is_empty() || control(text).is_some() {
            shared.pending.fetch_sub(1, Ordering::SeqCst);
            match control(text) {
                Some(said) => act(&shared, said),
                None if !shared.busy() => shared.show(Phase::Listening),
                None => {}
            }
            continue;
        }
        // A reply to the card on screen answers it now: queued, it would wait
        // behind the very run the card is holding up.
        if crate::agent::answer_by_voice(&shared.app, text) {
            finish(&shared);
            continue;
        }
        tracing::info!(request = %text, "always-listening request");
        let _ = requests.send((shared.generation.load(Ordering::SeqCst), text.to_string()));
    }
}

/// Runs requests one at a time, in the order they were said. Ends when the
/// listener and its turn taker (the senders) are gone.
async fn run_requests(shared: Arc<Shared>, mut queue: mpsc::UnboundedReceiver<(u64, String)>) {
    while let Some((generation, request)) = queue.recv().await {
        if generation != shared.generation.load(Ordering::SeqCst) {
            continue;
        }
        let cancelled = || shared.generation.load(Ordering::SeqCst) != generation;
        if let Err(e) =
            crate::agent::run(&shared.app, request, &shared.settings, true, cancelled).await
        {
            tracing::error!(%e, "always-listening request failed");
        }
        finish(&shared);
    }
}

/// One request is over: once none are left, the wings go back to listening.
/// Straight back, not via "done": that phase reads "Pasted", and an answer or
/// a question has the panel anyway.
fn finish(shared: &Shared) {
    let left = shared
        .pending
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
            Some(n.saturating_sub(1))
        })
        .map_or(0, |was| was.saturating_sub(1));
    if left == 0 {
        shared.show(Phase::Listening);
    }
}

/// Spoken session controls, matched whole so "cancel the task in Calculator"
/// is still a request. Jev Voice's phrases.
#[derive(Debug, PartialEq)]
enum Control {
    StopListening,
    Cancel,
}

fn control(text: &str) -> Option<Control> {
    let said = text
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_lowercase();
    match said.as_str() {
        "stop listening"
        | "turn off the mic"
        | "turn off microphone"
        | "microphone off"
        | "mic off" => Some(Control::StopListening),
        "cancel task" | "stop now" => Some(Control::Cancel),
        _ => None,
    }
}

fn act(shared: &Shared, said: Control) {
    match said {
        // With no bar, a mic that is off but a mode that is on would be
        // invisible: stopping listening ends the mode. Spawned, because this
        // runs inside the session it is about to drop.
        Control::StopListening => {
            let app = shared.app.clone();
            tauri::async_runtime::spawn(async move { crate::stop_listening(&app) });
        }
        Control::Cancel => shared.cancel(),
    }
}

fn open_mic(shared: &Arc<Shared>, turns: Turns) -> Result<aloud_audio::Recorder> {
    let device = shared.settings.selected_microphone.as_deref();
    let dictating = shared.pipeline.busy_flag();
    let key = shared.settings.deepgram_api_key.trim().to_string();

    if key.is_empty() {
        let mut segmenter = aloud_audio::Segmenter::new(SPEECH_RMS);
        return aloud_audio::Recorder::tap_only(
            device,
            Box::new(move |pcm: Vec<u8>| {
                if dictating.load(Ordering::Relaxed) {
                    segmenter.reset();
                    return;
                }
                // Send, never transcribe, on this thread: the tap must not block.
                for utterance in segmenter.push(&pcm) {
                    let _ = turns.send((None, utterance));
                }
            }),
        );
    }

    let (audio, heard) = mpsc::unbounded_channel();
    let turn = Arc::new(Mutex::new(Vec::new()));
    let recorder = aloud_audio::Recorder::tap_only(device, {
        let turn = turn.clone();
        Box::new(move |mut pcm: Vec<u8>| {
            if let Ok(mut turn) = turn.lock() {
                if dictating.load(Ordering::Relaxed) {
                    // A dictation is not part of the next request.
                    turn.clear();
                } else {
                    turn.extend_from_slice(&pcm);
                    let over = turn.len().saturating_sub(MAX_TURN_BYTES);
                    turn.drain(..over);
                }
            }
            // Silence rather than nothing while a dictation owns the mic:
            // Deepgram closes a stream that stops receiving audio.
            if dictating.load(Ordering::Relaxed) {
                pcm.fill(0);
            }
            let _ = audio.send(pcm);
        })
    })?;
    tauri::async_runtime::spawn(hear_deepgram(shared.clone(), key, heard, turn, turns));
    Ok(recorder)
}

/// Streams to Deepgram until the mic closes, reopening a dropped stream.
async fn hear_deepgram(
    shared: Arc<Shared>,
    key: String,
    mut audio: mpsc::UnboundedReceiver<Vec<u8>>,
    turn: Arc<Mutex<Vec<u8>>>,
    turns: Turns,
) {
    loop {
        let result = deepgram::stream(
            &key,
            aloud_audio::REALTIME_RATE,
            &shared.settings.vocabulary,
            &mut audio,
            |heard| match heard {
                // Live words only while nothing else is on screen: under
                // "transcribing" they would be the next request, not this one.
                Heard::Partial(text) => {
                    if !shared.busy() && !shared.dictating() {
                        crate::overlay::send_partial(&shared.app, &text);
                    }
                }
                Heard::Turn(text) => {
                    let said = turn
                        .lock()
                        .map(|mut t| std::mem::take(&mut *t))
                        .unwrap_or_default();
                    let _ = turns.send((Some(text), said));
                }
            },
        )
        .await;
        let Err(e) = result else {
            return; // the mic closed
        };
        tracing::warn!(error = %format!("{e:#}"), "deepgram stream dropped; reopening");
        tokio::time::sleep(RECONNECT).await;
        if audio.is_closed() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_match_whole_phrases_only() {
        assert_eq!(control("Stop listening."), Some(Control::StopListening));
        assert_eq!(control("  Cancel task!"), Some(Control::Cancel));
        assert_eq!(control("Cancel the task in Reminders"), None);
        assert_eq!(control("What is 90 plus 7?"), None);
    }
}
