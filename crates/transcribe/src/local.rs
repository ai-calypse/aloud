//! On-device transcription via whisper.cpp. Works on both platforms and needs
//! no network.

use crate::{hallucination, Transcriber};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Default model: ~150 MB, good accuracy-to-speed for dictation on CPU.
pub const DEFAULT_MODEL: &str = "ggml-base.en.bin";

pub struct LocalTranscriber {
    /// Shared so inference can move onto a blocking thread without borrowing
    /// `self` across the await.
    context: Arc<WhisperContext>,
    language: Option<String>,
}

impl LocalTranscriber {
    /// Loads a ggml model. Expensive — do this once and reuse.
    pub fn new(model_path: &Path, language: Option<&str>) -> Result<Self> {
        // whisper.cpp writes to stdout/stderr from C. Without the hooks, model
        // load takes 19s instead of 200ms and the log fills with junk.
        whisper_rs::install_logging_hooks();

        if !model_path.exists() {
            bail!("whisper model not found at {}", model_path.display());
        }
        let context =
            WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
                .with_context(|| format!("loading model {}", model_path.display()))?;

        Ok(Self {
            context: Arc::new(context),
            language: language
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string),
        })
    }
}

/// Threads for the CPU-side decode, leaving one core for the audio callback and
/// the event tap, which must not be starved.
fn threads() -> i32 {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    cores.saturating_sub(1).clamp(1, 8) as i32
}

/// Free-standing so it can run on the blocking pool without borrowing `self`.
fn run_whisper(
    context: &WhisperContext,
    language: Option<&str>,
    samples: &[f32],
) -> Result<String> {
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(language.or(Some("en")));
    // The transcript is returned, not printed.
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    // whisper.cpp defaults to `min(4, hardware_concurrency)`. Measured as a
    // no-op on macOS (Metal owns the encoder), kept for Windows, where a
    // 4-thread cap on a 16-core box is the whole decode.
    params.set_n_threads(threads());

    // A dictation is one utterance: priming the decoder with the previous
    // window's text only invites a run-on.
    params.set_no_context(true);

    let mut state = context.create_state().context("creating state")?;
    state.full(params, samples).context("running whisper")?;

    let segments = state.full_n_segments();
    let mut text = String::new();
    let mut no_speech_prob = None;

    for i in 0..segments {
        let segment = state.get_segment(i).context("reading segment")?;
        text.push_str(&segment.to_str_lossy().context("decoding segment")?);
        // Match the cloud path: judge on the first segment only.
        if i == 0 {
            no_speech_prob = Some(f64::from(segment.no_speech_probability()));
        }
    }

    let text = strip_non_speech_markers(&text);
    if hallucination::is_hallucination(&text, no_speech_prob) {
        tracing::info!(%text, "dropped hallucinated transcript");
        return Ok(String::new());
    }
    Ok(text)
}

#[async_trait]
impl Transcriber for LocalTranscriber {
    async fn transcribe(&self, wav_path: &Path) -> Result<String> {
        let samples = read_wav_16k_mono(wav_path)?;

        // Seconds of blocking CPU: on an async worker this starves the runtime
        // and the dictation hangs at "Transcribing", because the task that would
        // publish the result never gets scheduled.
        let context = self.context.clone();
        let language = self.language.clone();
        tokio::task::spawn_blocking(move || run_whisper(&context, language.as_deref(), &samples))
            .await
            .context("whisper task panicked")?
    }
}

/// Strips whisper.cpp's square-bracketed non-speech annotations (`[BLANK_AUDIO]`,
/// `[MUSIC]`, …), which must never reach the user's document. Square brackets
/// only: parentheses appear in ordinary dictated speech.
fn strip_non_speech_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find('[') {
        match rest[start..].find(']') {
            Some(end) => {
                out.push_str(&rest[..start]);
                rest = &rest[start + end + 1..];
            }
            // An unclosed bracket is not an annotation; keep the text.
            None => break,
        }
    }
    out.push_str(rest);

    // Markers leave double spaces behind when they sat mid-sentence.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Reads a PCM16 WAV as f32 samples. whisper.cpp requires 16 kHz mono.
fn read_wav_16k_mono(path: &Path) -> Result<Vec<f32>> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("opening {}", path.display()))?;
    let spec = reader.spec();

    if spec.sample_rate != 16_000 || spec.channels != 1 {
        bail!(
            "whisper needs 16kHz mono, got {}Hz {}ch",
            spec.sample_rate,
            spec.channels
        );
    }

    reader
        .samples::<i16>()
        .map(|s| Ok(f32::from(s?) / f32::from(i16::MAX)))
        .collect::<Result<Vec<f32>, hound::Error>>()
        .context("reading samples")
}

/// Whether the default model is already on disk. The offline fallback checks
/// this because `ensure_model` *downloads* a missing one, which would put 150MB
/// inside the dictation the user is waiting on — over the network that just
/// broke.
pub fn model_is_present() -> bool {
    model_dir().is_ok_and(|dir| dir.join(DEFAULT_MODEL).exists())
}

/// Where models live: `<app-data>/Aloud/models`.
pub fn model_dir() -> Result<PathBuf> {
    Ok(aloud_notes::data_dir()?.join("models"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_sample_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wrong.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        // Both channels: hound rejects a frame left half-written.
        w.write_sample(0i16).unwrap();
        w.write_sample(0i16).unwrap();
        w.finalize().unwrap();

        let Err(err) = read_wav_16k_mono(&path) else {
            panic!("a 44.1kHz stereo wav must be rejected");
        };
        assert!(err.to_string().contains("16kHz mono"), "got: {err}");
    }

    #[test]
    fn reads_16k_mono_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ok.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        w.write_sample(i16::MAX).unwrap();
        w.write_sample(0i16).unwrap();
        w.finalize().unwrap();

        let samples = read_wav_16k_mono(&path).unwrap();
        assert_eq!(samples.len(), 2);
        assert!((samples[0] - 1.0).abs() < 0.001);
    }

    /// whisper.cpp reports silence as a literal "[BLANK_AUDIO]", which is not
    /// empty and would otherwise be pasted into the user's document.
    #[test]
    fn strips_blank_audio_marker() {
        assert_eq!(strip_non_speech_markers("[BLANK_AUDIO]"), "");
        assert_eq!(strip_non_speech_markers(" [BLANK_AUDIO] "), "");
        assert_eq!(strip_non_speech_markers("[MUSIC][SOUND]"), "");
    }

    #[test]
    fn keeps_speech_around_markers() {
        assert_eq!(
            strip_non_speech_markers("Hello [MUSIC] world"),
            "Hello world",
            "a marker mid-sentence must not leave a double space"
        );
    }

    #[test]
    fn leaves_parentheses_and_unclosed_brackets_alone() {
        assert_eq!(
            strip_non_speech_markers("call foo(bar) then baz"),
            "call foo(bar) then baz"
        );
        assert_eq!(
            strip_non_speech_markers("array[0] is unclosed ["),
            "array is unclosed [",
            "a closed pair is stripped; a dangling bracket is kept"
        );
    }

    #[test]
    fn missing_model_errors_clearly() {
        let Err(err) = LocalTranscriber::new(Path::new("/nope/model.bin"), None) else {
            panic!("a missing model must not load");
        };
        assert!(err.to_string().contains("not found"), "got: {err}");
    }
}
