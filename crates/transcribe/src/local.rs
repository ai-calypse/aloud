//! On-device transcription via whisper.cpp.
//!
//! Replaces the Swift app's `SFSpeechRecognizer` offline fallback, which is
//! macOS-only. This works identically on Windows and needs no network.

use crate::{hallucination, Transcriber};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Default model: ~150 MB, good accuracy-to-speed for dictation on CPU.
pub const DEFAULT_MODEL: &str = "ggml-base.en.bin";

pub struct LocalTranscriber {
    context: WhisperContext,
    language: Option<String>,
}

impl LocalTranscriber {
    /// Loads a ggml model. Expensive — do this once and reuse.
    pub fn new(model_path: &Path, language: Option<&str>) -> Result<Self> {
        // whisper.cpp writes progress straight to stdout/stderr from C; route
        // it into tracing so it is filterable and does not corrupt app output.
        whisper_rs::install_logging_hooks();

        if !model_path.exists() {
            bail!("whisper model not found at {}", model_path.display());
        }
        let context =
            WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
                .with_context(|| format!("loading model {}", model_path.display()))?;

        Ok(Self {
            context,
            language: language
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string),
        })
    }

    /// Transcribes 16 kHz mono f32 samples.
    pub fn transcribe_samples(&self, samples: &[f32]) -> Result<String> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(self.language.as_deref().or(Some("en")));
        // The transcript is returned, not printed.
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        let mut state = self.context.create_state().context("creating state")?;
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

        let text = text.trim().to_string();
        if hallucination::is_hallucination(&text, no_speech_prob) {
            tracing::info!(%text, "dropped hallucinated transcript");
            return Ok(String::new());
        }
        Ok(text)
    }
}

#[async_trait]
impl Transcriber for LocalTranscriber {
    async fn transcribe(&self, wav_path: &Path) -> Result<String> {
        let samples = read_wav_16k_mono(wav_path)?;
        self.transcribe_samples(&samples)
    }
}

/// Reads a PCM16 WAV as f32 samples. whisper.cpp requires 16 kHz mono, which
/// is exactly what the audio crate writes.
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

    #[test]
    fn missing_model_errors_clearly() {
        let Err(err) = LocalTranscriber::new(Path::new("/nope/model.bin"), None) else {
            panic!("a missing model must not load");
        };
        assert!(err.to_string().contains("not found"), "got: {err}");
    }
}
