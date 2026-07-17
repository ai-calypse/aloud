//! Microphone capture, ported from `AudioRecorder.swift`.
//!
//! cpal replaces `AVCaptureSession`, so it works on CoreAudio (macOS) and
//! WASAPI (Windows) alike. Capture runs at the device's native rate and is
//! converted on stop:
//!
//! - a 16 kHz mono PCM16 WAV file for upload-based transcription
//! - 24 kHz mono PCM16 chunks for the realtime tap, matching the rate the
//!   Swift app streamed to OpenAI Realtime

pub mod resample;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use serde::Serialize;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

pub const TRANSCRIBE_RATE: u32 = 16_000;
pub const REALTIME_RATE: u32 = 24_000;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AudioDevice {
    /// Stable id, safe to persist in settings.
    pub id: String,
    pub name: String,
}

/// Every input device, sorted by name. Deduplicated by id — the Swift version
/// had to do the same, as macOS can list one physical mic more than once.
pub fn input_devices() -> Result<Vec<AudioDevice>> {
    let host = cpal::default_host();
    let mut devices: Vec<AudioDevice> = host
        .input_devices()
        .context("enumerating input devices")?
        .filter_map(|d| {
            Some(AudioDevice {
                id: d.id().ok()?.to_string(),
                name: d.description().ok()?.name().to_string(),
            })
        })
        .filter(|d| !d.id.is_empty() && !d.name.is_empty())
        .collect();

    devices.sort_by_key(|d| d.name.to_lowercase());
    devices.dedup_by(|a, b| a.id == b.id);
    Ok(devices)
}

/// The requested device, falling back to the system default when it's absent
/// (unplugged since the user picked it) or when `id` is None.
fn resolve_device(id: Option<&str>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    if let Some(id) = id.filter(|i| !i.is_empty() && *i != "default") {
        match id.parse().ok().and_then(|id| host.device_by_id(&id)) {
            Some(device) => return Ok(device),
            None => tracing::info!(%id, "selected mic unavailable, using system default"),
        }
    }
    host.default_input_device()
        .ok_or_else(|| anyhow!("no audio input device available"))
}

/// A capture session. Recording stops when this is dropped or `stop` is called.
pub struct Recorder {
    _stream: Stream,
    captured: Arc<Mutex<Vec<f32>>>,
    source_rate: u32,
}

impl Recorder {
    /// Opens `device_id` (or the default mic) and starts capturing.
    ///
    /// `on_pcm16` receives 24 kHz mono PCM16 chunks as they arrive, for the
    /// realtime transcription path. It runs on the audio callback thread, so
    /// it must not block.
    pub fn start(
        device_id: Option<&str>,
        on_pcm16: Option<Box<dyn FnMut(Vec<u8>) + Send>>,
    ) -> Result<Self> {
        let device = resolve_device(device_id)?;
        let supported = device
            .default_input_config()
            .context("querying default input config")?;
        let source_rate = supported.sample_rate();
        let channels = supported.channels();
        let config: StreamConfig = supported.config();

        tracing::info!(
            device = %device.description().map(|d| d.name().to_string()).unwrap_or_default(),
            source_rate,
            channels,
            format = ?supported.sample_format(),
            "starting capture"
        );

        let captured = Arc::new(Mutex::new(Vec::<f32>::new()));
        let sink = captured.clone();
        let mut tap = on_pcm16;

        let mut on_samples = move |samples: &[f32]| {
            let mono = resample::to_mono(samples, channels);
            if let Some(tap) = tap.as_mut() {
                match resample::resample_mono(&mono, source_rate, REALTIME_RATE) {
                    Ok(chunk) => tap(resample::to_pcm16_bytes(&chunk)),
                    Err(e) => tracing::warn!(%e, "realtime resample failed"),
                }
            }
            if let Ok(mut buf) = sink.lock() {
                buf.extend_from_slice(&mono);
            }
        };

        let on_error = |e| tracing::error!(%e, "audio stream error");

        // Devices differ on native format; normalize everything to f32.
        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _| on_samples(data),
                on_error,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _| {
                    let f: Vec<f32> = data.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                    on_samples(&f)
                },
                on_error,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                config,
                move |data: &[u16], _| {
                    let f: Vec<f32> = data
                        .iter()
                        .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                        .collect();
                    on_samples(&f)
                },
                on_error,
                None,
            ),
            other => return Err(anyhow!("unsupported sample format: {other:?}")),
        }
        .context("building input stream")?;

        // cpal 0.18 no longer auto-starts CoreAudio streams.
        stream.play().context("starting stream")?;

        Ok(Self {
            _stream: stream,
            captured,
            source_rate,
        })
    }

    /// Stops capture and writes a 16 kHz mono PCM16 WAV to `path`.
    pub fn stop_to_wav(self, path: &Path) -> Result<()> {
        let samples = self.take_samples()?;
        if samples.is_empty() {
            return Err(anyhow!("no audio was captured from the microphone"));
        }
        let resampled = resample::resample_mono(&samples, self.source_rate, TRANSCRIBE_RATE)?;
        if resampled.is_empty() {
            return Err(anyhow!(
                "resampling {} samples from {}Hz produced no audio",
                samples.len(),
                self.source_rate
            ));
        }
        write_wav(path, &resampled, TRANSCRIBE_RATE)
    }

    /// Stops capture and returns the raw mono samples at the device's rate.
    pub fn stop(self) -> Result<Vec<f32>> {
        self.take_samples()
    }

    fn take_samples(&self) -> Result<Vec<f32>> {
        let mut buf = self
            .captured
            .lock()
            .map_err(|_| anyhow!("audio buffer lock poisoned"))?;
        Ok(std::mem::take(&mut *buf))
    }
}

/// Writes mono f32 samples as a PCM16 WAV.
pub fn write_wav(path: &Path, samples: &[f32], rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).with_context(|| format!("creating {path:?}"))?;
    for sample in samples {
        writer.write_sample(resample::to_i16(*sample))?;
    }
    writer.finalize().context("finalizing wav")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_roundtrips_at_declared_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.wav");
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32 / 10.0).sin() * 0.5).collect();

        write_wav(&path, &samples, TRANSCRIBE_RATE).unwrap();

        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().sample_rate, TRANSCRIBE_RATE);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.len() as usize, samples.len());
    }

    /// Device enumeration must not error even on a machine with no mic (CI).
    #[test]
    fn input_devices_does_not_error() {
        assert!(input_devices().is_ok());
    }
}
