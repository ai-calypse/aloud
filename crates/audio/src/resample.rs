//! Sample-rate conversion and channel downmixing.

use anyhow::Result;
use rubato::{FastFixedIn, PolynomialDegree, Resampler};

/// Averages interleaved multi-channel frames down to mono.
pub fn to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let n = channels as usize;
    interleaved
        .chunks_exact(n)
        .map(|frame| frame.iter().sum::<f32>() / n as f32)
        .collect()
}

/// Resamples mono f32 from `from_rate` to `to_rate`.
///
/// `FastFixedIn` (polynomial, no FFT) deliberately: the FFT resamplers buffer
/// internally and emit nothing for the first chunk, silently producing empty
/// output for short utterances.
pub fn resample_mono(input: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>> {
    if from_rate == to_rate || input.is_empty() {
        return Ok(input.to_vec());
    }

    const CHUNK: usize = 1024;
    let mut resampler = FastFixedIn::<f32>::new(
        to_rate as f64 / from_rate as f64,
        1.0, // no dynamic ratio changes
        PolynomialDegree::Septic,
        CHUNK,
        1,
    )?;

    let expected = (input.len() as u64 * to_rate as u64 / from_rate as u64) as usize;
    let mut out = Vec::with_capacity(expected);

    // Pad the input so the final partial chunk is still fed through whole.
    let padded_len = input.len().div_ceil(CHUNK) * CHUNK;
    let mut padded = input.to_vec();
    padded.resize(padded_len, 0.0);

    for chunk in padded.chunks(CHUNK) {
        let resampled = resampler.process(&[chunk], None)?;
        out.extend_from_slice(&resampled[0]);
    }

    out.resize(expected, 0.0);
    Ok(out)
}

/// f32 samples in [-1.0, 1.0] to little-endian PCM16 bytes.
pub fn to_pcm16_bytes(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|s| to_i16(*s).to_le_bytes())
        .collect()
}

/// Clamps before scaling: f32 from a device can exceed [-1, 1], and casting
/// an out-of-range float to i16 is UB-adjacent garbage rather than a clip.
pub fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_passthrough() {
        let samples = [0.1, 0.2, 0.3];
        assert_eq!(to_mono(&samples, 1), samples);
    }

    #[test]
    fn stereo_averages_channels() {
        // L/R pairs: (0,1) -> 0.5, (1,0) -> 0.5
        assert_eq!(to_mono(&[0.0, 1.0, 1.0, 0.0], 2), vec![0.5, 0.5]);
    }

    #[test]
    fn pcm16_clamps_out_of_range() {
        assert_eq!(to_i16(2.0), i16::MAX);
        assert_eq!(to_i16(-2.0), -i16::MAX);
        assert_eq!(to_i16(0.0), 0);
    }

    #[test]
    fn pcm16_bytes_are_little_endian() {
        assert_eq!(to_pcm16_bytes(&[1.0]), i16::MAX.to_le_bytes().to_vec());
    }

    #[test]
    fn resample_is_noop_at_same_rate() {
        let samples = vec![0.1, 0.2, 0.3];
        assert_eq!(resample_mono(&samples, 16_000, 16_000).unwrap(), samples);
    }

    #[test]
    fn resample_halves_length_when_halving_rate() {
        let input: Vec<f32> = (0..4800).map(|i| (i as f32 / 50.0).sin()).collect();
        let out = resample_mono(&input, 48_000, 24_000).unwrap();
        assert_eq!(out.len(), 2400);
    }

    /// Regression: a one-shot `process()` returned zero samples for lengths
    /// that do not divide into the chunk size, writing empty WAVs.
    #[test]
    fn resample_48k_to_16k_keeps_audio() {
        let input: Vec<f32> = (0..95_744).map(|i| (i as f32 / 50.0).sin()).collect();
        let out = resample_mono(&input, 48_000, 16_000).unwrap();
        assert_eq!(out.len(), 95_744 / 3);
        assert!(
            out.iter().any(|s| s.abs() > 0.1),
            "output must not be silence"
        );
    }

    /// Any input shorter than one chunk must still produce audio.
    #[test]
    fn resample_short_input_keeps_audio() {
        let input: Vec<f32> = (0..100).map(|i| (i as f32 / 5.0).sin()).collect();
        let out = resample_mono(&input, 48_000, 16_000).unwrap();
        assert_eq!(out.len(), 33);
    }
}
