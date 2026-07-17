//! Lifetime dictation stats, ported from `DictationStats.swift`.

use chrono::{DateTime, Local, NaiveDate};
use serde::{Deserialize, Serialize};

/// Typing speed used to estimate time saved.
const TYPING_WPM: f64 = 40.0;
/// Speaking speed used to estimate time spent, since the recorder's real
/// duration is not available at the success hook.
const SPEAKING_WPM: f64 = 150.0;

/// See [`crate::history::Entry`] on why u64 exports as `number`: a lifetime
/// word count will not reach 2^53.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Stats {
    #[specta(type = specta_typescript::Number)]
    pub total_words: u64,
    #[specta(type = specta_typescript::Number)]
    pub total_sessions: u64,
    pub total_spoken_seconds: f64,
    pub current_streak_days: u32,
    pub best_streak_days: u32,
    /// Local calendar day of the last dictation, as `YYYY-MM-DD`.
    ///
    /// A string rather than a `NaiveDate` so it crosses the specta boundary
    /// as a plain TS string; [`Self::last_day`] parses it back.
    pub last_dictation_day: Option<String>,
}

impl Stats {
    /// Time saved versus typing at 40 wpm, minus time actually spent speaking.
    /// Clamped so a slow talker never "loses" time.
    pub fn time_saved_seconds(&self) -> f64 {
        (self.total_words as f64 / TYPING_WPM * 60.0 - self.total_spoken_seconds).max(0.0)
    }

    pub fn estimated_spoken_seconds(words: u64) -> f64 {
        words as f64 / SPEAKING_WPM * 60.0
    }

    /// Records one dictation. Consecutive calendar days extend the streak, a
    /// same-day repeat keeps it, any gap resets it to 1.
    ///
    /// `at` is injectable so the streak math is testable.
    pub fn record_session(&mut self, words: u64, seconds: f64, at: DateTime<Local>) {
        if words == 0 {
            return;
        }
        self.total_words += words;
        self.total_sessions += 1;
        self.total_spoken_seconds += seconds;

        // Compare calendar days, not elapsed hours: a DST shift makes
        // consecutive days 23 or 25 hours apart, and both must count as one.
        let today = at.date_naive();
        self.current_streak_days = match self.last_day() {
            Some(last) => match (today - last).num_days() {
                0 => self.current_streak_days.max(1),
                1 => self.current_streak_days + 1,
                _ => 1,
            },
            None => 1,
        };
        self.best_streak_days = self.best_streak_days.max(self.current_streak_days);
        self.last_dictation_day = Some(today.to_string());
    }

    /// The last dictation day, parsed. None if unset or unparseable.
    pub fn last_day(&self) -> Option<NaiveDate> {
        self.last_dictation_day.as_deref()?.parse().ok()
    }

    /// Words in a transcript, for `record_session`.
    pub fn word_count(text: &str) -> u64 {
        text.split_whitespace().count() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn day(year: i32, month: u32, day: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap()
    }

    #[test]
    fn first_session_starts_streak() {
        let mut stats = Stats::default();
        stats.record_session(10, 4.0, day(2026, 3, 1));

        assert_eq!(stats.total_words, 10);
        assert_eq!(stats.total_sessions, 1);
        assert_eq!(stats.current_streak_days, 1);
        assert_eq!(stats.best_streak_days, 1);
    }

    #[test]
    fn same_day_repeat_does_not_increment() {
        let mut stats = Stats::default();
        stats.record_session(10, 4.0, day(2026, 3, 1));
        stats.record_session(10, 4.0, day(2026, 3, 1));

        assert_eq!(stats.total_sessions, 2);
        assert_eq!(stats.current_streak_days, 1, "same day is still one day");
    }

    /// Consecutive days are 23 or 25 hours apart across a DST boundary, so the
    /// streak must be computed on calendar days, not elapsed time.
    #[test]
    fn consecutive_days_increment_across_dst() {
        let mut stats = Stats::default();
        stats.record_session(10, 4.0, day(2026, 3, 7));
        stats.record_session(10, 4.0, day(2026, 3, 8)); // US DST change
        stats.record_session(10, 4.0, day(2026, 3, 9));

        assert_eq!(stats.current_streak_days, 3);
    }

    #[test]
    fn gap_resets_streak_but_keeps_best() {
        let mut stats = Stats::default();
        stats.record_session(10, 4.0, day(2026, 3, 1));
        stats.record_session(10, 4.0, day(2026, 3, 2));
        stats.record_session(10, 4.0, day(2026, 3, 5)); // 3-day gap

        assert_eq!(stats.current_streak_days, 1);
        assert_eq!(stats.best_streak_days, 2, "best is a high-water mark");
    }

    #[test]
    fn time_saved_clamps_at_zero() {
        let mut stats = Stats::default();
        // Rambling: far more speaking time than the typing equivalent.
        stats.record_session(10, 600.0, day(2026, 3, 1));
        assert_eq!(stats.time_saved_seconds(), 0.0);
    }

    #[test]
    fn time_saved_is_positive_for_normal_speech() {
        let mut stats = Stats::default();
        let words = 100;
        stats.record_session(
            words,
            Stats::estimated_spoken_seconds(words),
            day(2026, 3, 1),
        );
        // 100 words: 150s to type, 40s to speak.
        assert!((stats.time_saved_seconds() - 110.0).abs() < 0.1);
    }

    #[test]
    fn empty_session_is_ignored() {
        let mut stats = Stats::default();
        stats.record_session(0, 5.0, day(2026, 3, 1));
        assert_eq!(stats, Stats::default(), "silence must not count");
    }

    #[test]
    fn counts_words() {
        assert_eq!(Stats::word_count("hello there  world "), 3);
        assert_eq!(Stats::word_count(""), 0);
    }
}
