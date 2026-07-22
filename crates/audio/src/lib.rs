//! Microphone capture via cpal (CoreAudio / WASAPI).
//!
//! Capture runs at the device's native rate and is converted on stop: a 16 kHz
//! mono PCM16 WAV for upload-based transcription, 24 kHz mono PCM16 chunks for
//! the realtime tap.

pub mod resample;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use serde::Serialize;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
};

pub const TRANSCRIBE_RATE: u32 = 16_000;
pub const REALTIME_RATE: u32 = 24_000;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AudioDevice {
    /// Stable id, safe to persist in settings.
    pub id: String,
    pub name: String,
}

/// Every input device, sorted by name. Deduplicated by id: macOS can list one
/// physical mic more than once.
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

/// The requested device, falling back to the system default when it is absent
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
    /// Most recent input level, as f32 bits in an atomic — the audio callback
    /// thread must never block, so this cannot be a mutex.
    level: Arc<AtomicU32>,
}

impl Recorder {
    /// Opens `device_id` (or the default mic) and starts capturing.
    ///
    /// `on_pcm16` receives 24 kHz mono PCM16 chunks for the realtime path. It
    /// runs on the audio callback thread, so it must not block.
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
        let level = Arc::new(AtomicU32::new(0));
        let level_sink = level.clone();
        let mut tap = on_pcm16;

        let mut on_samples = move |samples: &[f32]| {
            let mono = resample::to_mono(samples, channels);
            level_sink.store(loudness(&mono).to_bits(), Ordering::Relaxed);
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
            level,
        })
    }

    /// Current input level, 0.0–1.0, for the overlay's waveform. The handle
    /// stays live only as long as this `Recorder` — how the meter task knows
    /// to stop.
    pub fn level_handle(&self) -> Arc<AtomicU32> {
        self.level.clone()
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

/// Perceived loudness of a block, 0.0–1.0. RMS not peak (a click reads as a
/// spike), and the sqrt curve is the tuning that matters: speech sits near RMS
/// 0.05, which `rms * 4` alone maps to a dead-looking 0.2, the curve to 0.45.
pub fn loudness(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares: f32 = samples.iter().map(|s| s * s).sum();
    let rms = (sum_squares / samples.len() as f32).sqrt();
    (rms * 4.0).clamp(0.0, 1.0).sqrt()
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

/// Writes raw realtime-tap bytes (24 kHz mono PCM16) as a 16 kHz WAV. The rate
/// change is the point: local whisper hard-rejects anything but 16 kHz mono and
/// silently dropped every chunk handed to it at the tap's native 24 kHz.
pub fn write_pcm16_wav(path: &Path, pcm16: &[u8]) -> Result<()> {
    let samples: Vec<f32> = pcm16
        .chunks_exact(2)
        .map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])) / f32::from(i16::MAX))
        .collect();
    let samples = resample::resample_mono(&samples, REALTIME_RATE, TRANSCRIBE_RATE)?;
    write_wav(path, &samples, TRANSCRIBE_RATE)
}

/// Cuts a continuous byte stream into fixed-size chunks, numbered in order.
///
/// Tap buffers are never a whole chunk, so the leftover is carried across
/// calls. The index is what reorders chunks transcribed concurrently.
#[derive(Debug)]
pub struct Chunker {
    pending: Vec<u8>,
    next_index: usize,
    chunk_bytes: usize,
}

impl Chunker {
    pub fn new(chunk_bytes: usize) -> Self {
        Self {
            pending: Vec::new(),
            next_index: 0,
            chunk_bytes,
        }
    }

    /// Adds bytes and returns every whole chunk they completed. A Vec, not an
    /// Option: one buffer can complete several chunks and stranding the rest in
    /// `pending` loses them at the end of a meeting.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<(usize, Vec<u8>)> {
        self.pending.extend_from_slice(bytes);
        let mut chunks = Vec::new();
        while self.pending.len() >= self.chunk_bytes {
            let chunk: Vec<u8> = self.pending.drain(..self.chunk_bytes).collect();
            chunks.push((self.next_index, chunk));
            self.next_index += 1;
        }
        chunks
    }

    /// The unfinished remainder and the index it would occupy, for the end of
    /// a stream. Leaves the chunker empty.
    pub fn take_tail(&mut self) -> (usize, Vec<u8>) {
        (self.next_index, std::mem::take(&mut self.pending))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dropping the leftover loses audio; double-counting it repeats it.
    #[test]
    fn chunker_carries_the_remainder_across_pushes() {
        let mut chunker = Chunker::new(10);
        assert!(chunker.push(&[0; 4]).is_empty(), "4 of 10 is not a chunk");
        assert!(chunker.push(&[0; 5]).is_empty(), "9 of 10 is not a chunk");

        let chunks = chunker.push(&[0; 3]); // 12 total
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, 0);
        assert_eq!(chunks[0].1.len(), 10);

        // The 2 bytes past the boundary must still be there.
        assert_eq!(chunker.take_tail(), (1, vec![0; 2]));
    }

    /// Emitting only the first chunk strands the rest — lost at meeting end.
    #[test]
    fn chunker_emits_every_chunk_a_single_push_completes() {
        let mut chunker = Chunker::new(4);
        let chunks = chunker.push(&[7; 14]);
        assert_eq!(chunks.len(), 3);
        assert_eq!(
            chunks.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 1, 2],
            "indexes must be consecutive and start at zero"
        );
        assert_eq!(chunker.take_tail(), (3, vec![7; 2]));
    }

    /// An exact multiple must not double-consume the boundary byte.
    #[test]
    fn chunker_handles_an_exact_multiple() {
        let mut chunker = Chunker::new(8);
        assert_eq!(chunker.push(&[1; 16]).len(), 2);
        assert_eq!(chunker.take_tail(), (2, Vec::new()));
    }

    #[test]
    fn chunker_preserves_byte_order_across_the_boundary() {
        let mut chunker = Chunker::new(4);
        let chunks = chunker.push(&[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(chunks[0].1, vec![1, 2, 3, 4]);
        assert_eq!(chunks[1].1, vec![5, 6, 7, 8]);
    }

    /// Regression: local whisper silently dropped every chunk staged at the
    /// tap's native 24 kHz. Staged WAVs must be 16 kHz mono.
    #[test]
    fn pcm16_wav_is_16k_mono_and_not_silent() {
        let rate = REALTIME_RATE as usize;
        let samples: Vec<f32> = (0..rate) // 1s of 440 Hz at 24 kHz
            .map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / rate as f32).sin())
            .collect();
        let pcm16 = resample::to_pcm16_bytes(&samples);

        let path = std::env::temp_dir().join("aloud-test-pcm16-chunk.wav");
        write_pcm16_wav(&path, &pcm16).unwrap();

        let mut reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!((spec.sample_rate, spec.channels), (16_000, 1));
        let peak = reader
            .samples::<i16>()
            .map(|s| s.unwrap().unsigned_abs())
            .max()
            .unwrap();
        assert!(peak > 8000, "resampled audio must not be near-silence");
        let _ = std::fs::remove_file(&path);
    }

    /// Guards the gain curve: silence reads zero, speech lands mid-range.
    #[test]
    fn loudness_separates_silence_from_speech() {
        assert_eq!(loudness(&[]), 0.0);
        assert_eq!(loudness(&[0.0; 512]), 0.0);

        let speech = loudness(&[0.05; 512]);
        assert!(speech > 0.35 && speech < 0.75, "got {speech}");
        assert!(loudness(&[0.1; 512]) > speech);
        assert_eq!(loudness(&[1.0; 512]), 1.0);
        assert!(loudness(&[-0.5; 512]) > 0.0, "sign must not matter");
    }

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

    /// Must not error on a machine with no mic (CI).
    #[test]
    fn input_devices_does_not_error() {
        assert!(input_devices().is_ok());
    }
}
