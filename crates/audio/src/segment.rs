//! Cutting an always-on mic stream into utterances at the pauses between them.

use std::collections::VecDeque;

const BYTES_PER_SECOND: usize = crate::REALTIME_RATE as usize * 2; // i16
/// 30 ms: short enough to catch a word's onset, long enough for a stable RMS.
const FRAME_BYTES: usize = BYTES_PER_SECOND * 30 / 1000;
const fn frames(ms: usize) -> usize {
    ms / 30
}
/// Kept from before the onset, or the first consonant is clipped and whisper
/// mishears the trigger phrase.
const PRE_ROLL_FRAMES: usize = frames(300);
/// A breath mid-sentence is shorter than this; the end of a request is longer.
const END_SILENCE_FRAMES: usize = frames(900);
/// A cough or a door slam is shorter than this and is not sent to whisper,
/// which answers noise with invented sentences.
const MIN_VOICED_FRAMES: usize = frames(300);
/// Nobody talks for this long without a pause; a TV or a fan does.
const MAX_BYTES: usize = BYTES_PER_SECOND * 30;

/// Splits 24 kHz mono PCM16 (the realtime tap) into utterances, each with a
/// little audio from before it began.
///
/// ponytail: a fixed RMS gate. A room loud enough to hold it open is caught by
/// `MAX_BYTES` rather than handled; an adaptive noise floor or Silero VAD is
/// the upgrade path if that turns out to be common.
#[derive(Debug)]
pub struct Segmenter {
    /// RMS, 0.0–1.0, a frame must reach to count as speech. The calibration
    /// knob: speech near the mic sits around 0.05, a quiet room under 0.005.
    threshold: f32,
    partial: Vec<u8>,
    pre_roll: VecDeque<Vec<u8>>,
    /// Some while an utterance is open.
    current: Option<Vec<u8>>,
    voiced: usize,
    silent: usize,
}

impl Segmenter {
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold,
            partial: Vec::new(),
            pre_roll: VecDeque::new(),
            current: None,
            voiced: 0,
            silent: 0,
        }
    }

    /// Feeds tap bytes and returns every utterance they finished.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.partial.extend_from_slice(bytes);
        let mut done = Vec::new();
        while self.partial.len() >= FRAME_BYTES {
            let frame: Vec<u8> = self.partial.drain(..FRAME_BYTES).collect();
            if let Some(utterance) = self.frame(frame) {
                done.push(utterance);
            }
        }
        done
    }

    /// Drops whatever is open, e.g. while the mic is someone else's to use.
    pub fn reset(&mut self) {
        *self = Self::new(self.threshold);
    }

    fn frame(&mut self, frame: Vec<u8>) -> Option<Vec<u8>> {
        let loud = rms(&frame) >= self.threshold;
        let Some(current) = self.current.as_mut() else {
            self.pre_roll.push_back(frame);
            if self.pre_roll.len() > PRE_ROLL_FRAMES {
                self.pre_roll.pop_front();
            }
            if loud {
                self.current = Some(self.pre_roll.drain(..).flatten().collect());
                self.voiced = 1;
                self.silent = 0;
            }
            return None;
        };

        current.extend_from_slice(&frame);
        if loud {
            self.voiced += 1;
            self.silent = 0;
        } else {
            self.silent += 1;
        }
        if self.silent < END_SILENCE_FRAMES && current.len() < MAX_BYTES {
            return None;
        }
        let utterance = self.current.take()?;
        (self.voiced >= MIN_VOICED_FRAMES).then_some(utterance)
    }
}

fn rms(pcm16: &[u8]) -> f32 {
    let samples = pcm16.len() / 2;
    if samples == 0 {
        return 0.0;
    }
    let sum: f32 = pcm16
        .chunks_exact(2)
        .map(|pair| {
            let s = f32::from(i16::from_le_bytes([pair[0], pair[1]])) / f32::from(i16::MAX);
            s * s
        })
        .sum();
    (sum / samples as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const THRESHOLD: f32 = 0.02;

    /// `ms` of a constant-amplitude signal; 0.1 reads as speech, 0.0 as silence.
    fn audio(ms: usize, amplitude: f32) -> Vec<u8> {
        let sample = ((amplitude * f32::from(i16::MAX)) as i16).to_le_bytes();
        // Alternating sign so it is a tone, not DC.
        (0..BYTES_PER_SECOND / 2 * ms / 1000)
            .flat_map(|i| {
                if i % 2 == 0 {
                    sample
                } else {
                    (-i16::from_le_bytes(sample)).to_le_bytes()
                }
            })
            .collect()
    }

    #[test]
    fn silence_yields_nothing() {
        let mut s = Segmenter::new(THRESHOLD);
        assert!(s.push(&audio(5_000, 0.0)).is_empty());
    }

    #[test]
    fn speech_then_a_pause_is_one_utterance_including_the_pre_roll() {
        let mut s = Segmenter::new(THRESHOLD);
        let mut out = s.push(&audio(1_000, 0.0));
        out.extend(s.push(&audio(1_200, 0.1)));
        out.extend(s.push(&audio(1_500, 0.0)));
        assert_eq!(out.len(), 1);
        // Pre-roll + speech + the silence that closed it.
        let expected = (PRE_ROLL_FRAMES + frames(1_200) + END_SILENCE_FRAMES) * FRAME_BYTES;
        assert_eq!(out[0].len(), expected);
    }

    /// A breath mid-request must not cut the command in half.
    #[test]
    fn a_short_pause_does_not_split_the_utterance() {
        let mut s = Segmenter::new(THRESHOLD);
        let mut out = s.push(&audio(800, 0.1));
        out.extend(s.push(&audio(400, 0.0)));
        out.extend(s.push(&audio(800, 0.1)));
        out.extend(s.push(&audio(1_500, 0.0)));
        assert_eq!(out.len(), 1);
    }

    /// Whisper turns a click into "Thank you." — it must never reach it.
    #[test]
    fn a_click_is_not_an_utterance() {
        let mut s = Segmenter::new(THRESHOLD);
        let mut out = s.push(&audio(60, 0.5));
        out.extend(s.push(&audio(2_000, 0.0)));
        assert!(out.is_empty());
    }

    #[test]
    fn noise_that_never_pauses_is_cut_at_the_cap() {
        let mut s = Segmenter::new(THRESHOLD);
        let out = s.push(&audio(65_000, 0.1));
        assert_eq!(out.len(), 2);
        assert!(out
            .iter()
            .all(|u| u.len() >= MAX_BYTES && u.len() < MAX_BYTES + FRAME_BYTES * 20));
    }

    #[test]
    fn reset_drops_an_open_utterance() {
        let mut s = Segmenter::new(THRESHOLD);
        s.push(&audio(1_000, 0.1));
        s.reset();
        assert!(s.push(&audio(2_000, 0.0)).is_empty());
    }
}
