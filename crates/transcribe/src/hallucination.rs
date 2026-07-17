//! Filters Whisper's canned hallucinations on silence.
//!
//! Ported verbatim from `TranscriptionService.swift`. Whisper-large-v3
//! confidently emits stock YouTube-outro phrases when it hears near-silence,
//! so they're dropped when whisper itself reports a high `no_speech_prob`.
//!
//! The Swift comment notes these thresholds were tuned on ~500 samples across
//! quiet and noisy environments, deliberately conservative to avoid filtering
//! real speech ("thank you" said aloud has a very low no_speech_prob). Do not
//! retune without the same evidence.

/// Phrases Whisper invents on silence. Add a phrase here to filter more.
const HALLUCINATION_PHRASES: &[&str] = &[
    "thank you",
    "thank you for watching",
    "thank you very much",
    "thank you so much",
    "thanks for watching",
    "please subscribe",
    "like and subscribe",
    "subtitles by",
    "subtitles by the amara.org community",
    "you",
];

/// Above this, whisper is telling us the audio was probably silence.
const NO_SPEECH_THRESHOLD: f64 = 0.1;

/// Whether `text` is a hallucination, given whisper's own no-speech
/// probability for the first segment.
///
/// `no_speech_prob` is None when the provider omitted segment metadata (the
/// `gpt-4o-transcribe` family only returns plain JSON). Without it the filter
/// cannot judge, so the text is kept — dropping real speech is worse than
/// letting a stray "thank you" through.
pub fn is_hallucination(text: &str, no_speech_prob: Option<f64>) -> bool {
    let normalized = normalize(text);
    if !HALLUCINATION_PHRASES.contains(&normalized.as_str()) {
        return false;
    }

    match no_speech_prob {
        Some(prob) => prob >= NO_SPEECH_THRESHOLD,
        None => {
            tracing::info!(
                phrase = %normalized,
                "skipping hallucination filter: provider omitted no_speech_prob"
            );
            false
        }
    }
}

/// Lowercased, stripped of surrounding punctuation and whitespace, so
/// "Thank you." matches "thank you".
fn normalize(text: &str) -> String {
    text.trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_stock_phrase_on_silence() {
        assert!(is_hallucination("Thank you.", Some(0.9)));
        assert!(is_hallucination("thanks for watching", Some(0.5)));
    }

    /// The false-positive guard: "thank you" actually spoken has a very low
    /// no_speech_prob and must survive.
    #[test]
    fn keeps_stock_phrase_when_really_spoken() {
        assert!(!is_hallucination("Thank you.", Some(0.01)));
    }

    #[test]
    fn keeps_normal_speech_regardless_of_probability() {
        assert!(!is_hallucination("send the report by friday", Some(0.99)));
    }

    /// Providers without segment metadata cannot be judged; keep the text.
    #[test]
    fn keeps_text_when_probability_is_unknown() {
        assert!(!is_hallucination("Thank you.", None));
    }

    #[test]
    fn normalizes_case_and_punctuation() {
        assert_eq!(normalize("  Thank You!  "), "thank you");
        assert_eq!(normalize("You?"), "you");
    }

    #[test]
    fn threshold_is_inclusive() {
        assert!(is_hallucination("you", Some(NO_SPEECH_THRESHOLD)));
        assert!(!is_hallucination("you", Some(NO_SPEECH_THRESHOLD - 0.001)));
    }
}
