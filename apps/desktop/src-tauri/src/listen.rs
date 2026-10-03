//! Always-listening mode, after Jev Voice: a floating bar, a mic that stays
//! open, and every spoken request run as an agent task. Requests said while one
//! is running queue behind it — "open Calculator", then "what's 90 + 7", which
//! runs in the Calculator now in front.
//!
//! Turning the mode on is the activation; there is no per-request trigger.
//! Room speech can therefore start a run, which is bounded the way every run
//! is: each connected-account write and each risky click waits for a yes.
//!
//! Hearing is Deepgram's Flux when a key is set — live words in the bar, and
//! a turn end that understands an unfinished sentence — else a pause detector
//! feeding local whisper, so a bundled build without a `.env` still works.

use aloud_store::Settings;
use aloud_transcribe::deepgram::{self, Heard};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, LogicalPosition, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_specta::Event;
use tokio::sync::mpsc;

use crate::AppState;

/// The bar's window.
pub const LABEL: &str = "listen";
/// Jev Voice's command bar, point for point.
const BAR_WIDTH: f64 = 488.0;
const BAR_HEIGHT: f64 = 60.0;
/// Centred under the menu bar, where Jev Voice puts it.
const BAR_TOP_GAP: f64 = 12.0;
/// Speech RMS a frame must reach in the whisper fallback. See `Segmenter`.
const SPEECH_RMS: f32 = 0.02;
/// Wait before reopening a dropped Deepgram stream.
const RECONNECT: Duration = Duration::from_secs(2);

/// What the bar draws, sent whole on every change: it is small, and a bar that
/// loads late or misses one still ends up right.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "listen-state")]
pub struct ListenState {
    pub mic: bool,
    /// What is being said now, else the last request heard.
    pub transcript: String,
    pub busy: bool,
    /// Requests waiting behind the running one.
    pub queued: u32,
    /// Why the mic is not hearing, e.g. a dropped stream being reopened.
    pub problem: Option<String>,
}

/// What every part of a session shares. Holds no request sender, or the runner
/// would keep its own channel open and never end.
struct Shared {
    app: AppHandle,
    settings: Settings,
    /// The pipeline's: while a hotkey dictation owns the mic, what is said is
    /// a dictation, not a request.
    dictating: Arc<AtomicBool>,
    state: Mutex<ListenState>,
    /// Bumped by a cancel: the running request stops between tools and the
    /// queued ones, stamped with an older value, are skipped.
    generation: AtomicU64,
}

type Requests = mpsc::UnboundedSender<(u64, String)>;

impl Shared {
    fn update(&self, change: impl FnOnce(&mut ListenState)) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let before = state.clone();
        change(&mut state);
        // Flux repeats an unchanged partial four times a second.
        if *state != before {
            let _ = state.emit_to(&self.app, LABEL);
        }
    }

    fn submit(&self, requests: &Requests, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.update(|s| {
            s.transcript = text.to_string();
            s.queued += 1;
        });
        let _ = requests.send((self.generation.load(Ordering::SeqCst), text.to_string()));
    }

    fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.update(|s| s.queued = 0);
    }
}

/// A running session. Dropping it closes the mic, stops the running request,
/// skips the queue and closes the bar.
pub struct Listener {
    shared: Arc<Shared>,
    requests: Requests,
    /// Some while the mic is on. Dropping it ends the hearing task with it.
    mic: Option<aloud_audio::Recorder>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(bar) = self.shared.app.get_webview_window(LABEL) {
            let _ = bar.destroy();
        }
    }
}

impl Listener {
    pub fn start(app: AppHandle, settings: Settings, dictating: Arc<AtomicBool>) -> Result<Self> {
        let (requests, queue) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            app,
            settings,
            dictating,
            state: Mutex::default(),
            generation: AtomicU64::new(0),
        });
        tauri::async_runtime::spawn(run_requests(shared.clone(), queue));
        open_bar(&shared.app).context("opening the listening bar")?;
        let mut listener = Self {
            shared,
            requests,
            mic: None,
        };
        listener.set_mic(true);
        Ok(listener)
    }

    pub fn set_mic(&mut self, on: bool) {
        if on == self.mic.is_some() {
            return;
        }
        self.mic = None;
        if !on {
            self.shared.update(|s| {
                s.mic = false;
                s.problem = None;
            });
            return;
        }
        match open_mic(&self.shared, &self.requests) {
            Ok(recorder) => {
                self.mic = Some(recorder);
                self.shared.update(|s| {
                    s.mic = true;
                    s.problem = None;
                });
            }
            Err(e) => {
                tracing::error!(error = %format!("{e:#}"), "always-listening mic failed");
                self.shared.update(|s| {
                    s.mic = false;
                    s.problem = Some(format!("{e:#}"));
                });
            }
        }
    }
}

/// Runs requests one at a time, in the order they were said. Ends when the
/// listener and every hearing task (the senders) are gone.
async fn run_requests(shared: Arc<Shared>, mut queue: mpsc::UnboundedReceiver<(u64, String)>) {
    while let Some((generation, request)) = queue.recv().await {
        if generation != shared.generation.load(Ordering::SeqCst) {
            continue;
        }
        shared.update(|s| {
            s.queued = s.queued.saturating_sub(1);
            s.busy = true;
            s.transcript = request.clone();
        });
        tracing::info!(%request, "always-listening request");
        let cancelled = || shared.generation.load(Ordering::SeqCst) != generation;
        if let Err(e) =
            crate::agent::run(&shared.app, request, &shared.settings, true, cancelled).await
        {
            tracing::error!(%e, "always-listening request failed");
        }
        shared.update(|s| s.busy = false);
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

fn on_heard(shared: &Shared, requests: &Requests, heard: Heard) {
    match heard {
        // An empty partial is the gap between turns; the last request stays up.
        Heard::Partial(text) => shared.update(|s| {
            s.problem = None;
            if !text.is_empty() {
                s.transcript = text;
            }
        }),
        Heard::Turn(text) => match control(&text) {
            Some(Control::StopListening) => set_mic(&shared.app, false),
            Some(Control::Cancel) => shared.cancel(),
            None => shared.submit(requests, &text),
        },
    }
}

fn open_mic(shared: &Arc<Shared>, requests: &Requests) -> Result<aloud_audio::Recorder> {
    let device = shared.settings.selected_microphone.as_deref();
    let dictating = shared.dictating.clone();
    let key = shared.settings.deepgram_api_key.trim().to_string();

    if key.is_empty() {
        let (utterances, heard) = mpsc::unbounded_channel();
        let mut segmenter = aloud_audio::Segmenter::new(SPEECH_RMS);
        let recorder = aloud_audio::Recorder::tap_only(
            device,
            Box::new(move |pcm: Vec<u8>| {
                if dictating.load(Ordering::Relaxed) {
                    segmenter.reset();
                    return;
                }
                // Send, never transcribe, on this thread: the tap must not block.
                for utterance in segmenter.push(&pcm) {
                    let _ = utterances.send(utterance);
                }
            }),
        )?;
        tauri::async_runtime::spawn(hear_whisper(shared.clone(), requests.clone(), heard));
        return Ok(recorder);
    }

    let (audio, heard) = mpsc::unbounded_channel();
    let recorder = aloud_audio::Recorder::tap_only(
        device,
        Box::new(move |mut pcm: Vec<u8>| {
            // Silence rather than nothing while a dictation owns the mic:
            // Deepgram closes a stream that stops receiving audio.
            if dictating.load(Ordering::Relaxed) {
                pcm.fill(0);
            }
            let _ = audio.send(pcm);
        }),
    )?;
    tauri::async_runtime::spawn(hear_deepgram(shared.clone(), requests.clone(), key, heard));
    Ok(recorder)
}

/// Streams to Deepgram until the mic closes, reopening a dropped stream.
async fn hear_deepgram(
    shared: Arc<Shared>,
    requests: Requests,
    key: String,
    mut audio: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    loop {
        let result = deepgram::stream(
            &key,
            aloud_audio::REALTIME_RATE,
            &shared.settings.vocabulary,
            &mut audio,
            |heard| on_heard(&shared, &requests, heard),
        )
        .await;
        let Err(e) = result else {
            return; // the mic closed
        };
        tracing::warn!(error = %format!("{e:#}"), "deepgram stream dropped; reopening");
        shared.update(|s| s.problem = Some(format!("Reconnecting \u{b7} {e}")));
        tokio::time::sleep(RECONNECT).await;
        if audio.is_closed() {
            return;
        }
    }
}

/// Transcribes each paused-at utterance with the app's transcriber.
async fn hear_whisper(
    shared: Arc<Shared>,
    requests: Requests,
    mut utterances: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    // Built on the first utterance, as meetings do: a silent room never pays
    // for loading the model.
    let mut transcriber = None;
    while let Some(utterance) = utterances.recv().await {
        if transcriber.is_none() {
            match crate::pipeline::build_transcriber(&shared.app, &shared.settings).await {
                Ok(t) => transcriber = Some(t),
                Err(e) => {
                    tracing::error!(%e, "always-listening transcriber unavailable");
                    continue;
                }
            }
        }
        let Some(t) = transcriber.as_deref() else {
            continue;
        };
        match aloud_transcribe::preview::transcribe_pcm16(
            t,
            &utterance,
            &shared.settings.vocabulary,
        )
        .await
        {
            Ok(text) => on_heard(&shared, &requests, Heard::Turn(text)),
            Err(e) => tracing::warn!(%e, "always-listening transcription failed"),
        }
    }
}

fn open_bar(app: &AppHandle) -> Result<()> {
    let bar = match app.get_webview_window(LABEL) {
        Some(bar) => bar,
        None => WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("listen.html".into()))
            .title("Aloud")
            .inner_size(BAR_WIDTH, BAR_HEIGHT)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(true)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .focused(false)
            .accept_first_mouse(true)
            .visible(false)
            .build()?,
    };
    // ponytail: the primary display only; the overlay's notch targeting is the
    // model if the bar should follow the pointer's screen.
    if let Some(monitor) = bar.primary_monitor()? {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let left = f64::from(area.position.x) / scale;
        let width = f64::from(area.size.width) / scale;
        let top = f64::from(area.position.y) / scale;
        bar.set_position(LogicalPosition::new(
            left + (width - BAR_WIDTH) / 2.0,
            top + BAR_TOP_GAP,
        ))?;
    }
    bar.show()?;
    Ok(())
}

/// Runs `f` on the session, if one is on.
fn with_listener(app: &AppHandle, f: impl FnOnce(&mut Listener)) {
    let state = app.state::<AppState>();
    let Ok(mut slot) = state.listener.lock() else {
        return;
    };
    if let Some(listener) = slot.as_mut() {
        f(listener);
    }
}

fn set_mic(app: &AppHandle, on: bool) {
    with_listener(app, |l| l.set_mic(on));
}

/// The bar's first paint: it may load after the session's first update.
#[tauri::command]
#[specta::specta]
pub fn listen_state(state: State<'_, AppState>) -> ListenState {
    state
        .listener
        .lock()
        .ok()
        .and_then(|slot| {
            slot.as_ref()
                .and_then(|l| l.shared.state.lock().ok().map(|s| s.clone()))
        })
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub fn listen_toggle_mic(app: AppHandle) {
    with_listener(&app, |l| l.set_mic(l.mic.is_none()));
}

/// A typed request, queued like a spoken one.
#[tauri::command]
#[specta::specta]
pub fn listen_submit(app: AppHandle, text: String) {
    with_listener(&app, |l| l.shared.submit(&l.requests, &text));
}

/// Stops the running request and drops the queue; the mic stays on.
#[tauri::command]
#[specta::specta]
pub fn listen_cancel(app: AppHandle) {
    with_listener(&app, |l| l.shared.cancel());
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
