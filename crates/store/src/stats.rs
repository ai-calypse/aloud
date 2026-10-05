//! Lifetime dictation stats.

use crate::history::History;
use anyhow::Result;
use chrono::{DateTime, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Typing speed used to estimate time saved.
const TYPING_WPM: f64 = 40.0;
/// Speaking speed used to estimate time spent, since the recorder's real
/// duration is not available at the success hook.
const SPEAKING_WPM: f64 = 150.0;

/// Seconds an average completed agent command saves over doing it by hand.
///
/// An assumption, not a measurement: nothing records how long the same task
/// takes manually. It is weighted toward the common short commands. Opening an
/// app or pausing the music is ~15 s by hand; creating a note or reminder,
/// drafting an email or finding a setting is 2-3 min. Weighting that mix gives
/// about a minute and a half. One constant because history keeps no step or
/// tool counts to weight by; change it here and the Voice tile follows.
pub const SECONDS_SAVED_PER_ACTION: f64 = 90.0;

/// Time saved versus typing at `TYPING_WPM`, minus time spent speaking, clamped
/// so a slow talker never "loses" time. The one dictation formula: the lifetime
/// total and today's both go through it.
pub fn dictation_saved_seconds(words: u64, spoken_seconds: f64) -> f64 {
    (words as f64 / TYPING_WPM * 60.0 - spoken_seconds).max(0.0)
}

/// Time saved, split by where it came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TimeSaved {
    /// Dictation plus actions.
    pub total_seconds: f64,
    pub dictation_seconds: f64,
    pub action_seconds: f64,
    /// Agent commands that completed.
    pub actions: u32,
}

impl TimeSaved {
    pub fn new(dictation_seconds: f64, actions: u64) -> Self {
        let action_seconds = actions as f64 * SECONDS_SAVED_PER_ACTION;
        Self {
            total_seconds: dictation_seconds + action_seconds,
            dictation_seconds,
            action_seconds,
            actions: u32::try_from(actions).unwrap_or(u32::MAX),
        }
    }
}

/// Today and all time, for the Agents › Voice tile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TimeSavedView {
    pub all_time: TimeSaved,
    pub today: TimeSaved,
}

/// Lifetime dictation time comes from `stats` (it outlives the history cap);
/// actions, and today's dictation, from the history rows. A day is the local
/// calendar day, as the streak's is.
pub fn time_saved(stats: &Stats, history: &History, now: DateTime<Local>) -> Result<TimeSavedView> {
    let midnight = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|m| m.and_local_timezone(Local).earliest())
        .map_or(i64::MIN, |m| m.timestamp());
    let (_, all_actions) = history.voice_activity_since(i64::MIN)?;
    let (words_today, actions_today) = history.voice_activity_since(midnight)?;
    Ok(TimeSavedView {
        all_time: TimeSaved::new(stats.time_saved_seconds(), all_actions),
        today: TimeSaved::new(
            dictation_saved_seconds(words_today, Stats::estimated_spoken_seconds(words_today)),
            actions_today,
        ),
    })
}

/// See [`crate::history::Entry`] on why u64 exports as `number`.
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
    /// Local calendar day of the last dictation, as `YYYY-MM-DD`. A string so it
    /// crosses the specta boundary; [`Self::last_day`] parses it back.
    pub last_dictation_day: Option<String>,
}

impl Stats {
    /// `<app-data>/Aloud/stats.json`. Its own file, not a corner of
    /// settings.json: rewritten after every dictation, and a torn write must not
    /// take the API keys with it.
    pub fn path() -> Result<PathBuf> {
        Ok(aloud_notes::data_dir()?.join("stats.json"))
    }

    /// Lifetime stats from disk, or zeroes when absent or corrupt — losing a
    /// streak must never stop the app starting.
    pub fn load() -> Self {
        Self::path()
            .map(|p| Self::load_from(&p))
            .unwrap_or_default()
    }

    /// Temp + rename: a crash mid-write would otherwise zero a lifetime streak.
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    // Path-injectable halves, so tests never touch the real stats.json.

    fn load_from(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!(%e, "stats unreadable; starting from zero");
            Self::default()
        })
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let temp = path.with_extension("tmp");
        fs::write(&temp, serde_json::to_string_pretty(self)?)?;
        fs::rename(&temp, path)?;
        Ok(())
    }

    /// Time saved versus typing at 40 wpm, minus time actually spent speaking.
    /// Clamped so a slow talker never "loses" time.
    pub fn time_saved_seconds(&self) -> f64 {
        dictation_saved_seconds(self.total_words, self.total_spoken_seconds)
    }

    pub fn estimated_spoken_seconds(words: u64) -> f64 {
        words as f64 / SPEAKING_WPM * 60.0
    }

    /// Records one dictation. Consecutive calendar days extend the streak, a
    /// same-day repeat keeps it, any gap resets it to 1.
    pub fn record_session(&mut self, words: u64, seconds: f64, at: DateTime<Local>) {
        if words == 0 {
            return;
        }
        self.total_words += words;
        self.total_sessions += 1;
        self.total_spoken_seconds += seconds;

        // Calendar days, not elapsed hours: a DST shift makes consecutive days
        // 23 or 25 hours apart, and both must count as one.
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

    pub fn last_day(&self) -> Option<NaiveDate> {
        self.last_dictation_day.as_deref()?.parse().ok()
    }

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
    fn an_action_saves_the_assumed_time_and_adds_to_dictation() {
        let saved = TimeSaved::new(110.0, 4);
        assert_eq!(saved.action_seconds, 4.0 * SECONDS_SAVED_PER_ACTION);
        assert_eq!(saved.total_seconds, 110.0 + 360.0);
        assert_eq!(saved.actions, 4);
        assert_eq!(TimeSaved::new(0.0, 0), TimeSaved::default());
    }

    /// Guards: today's dictation and the lifetime total drifting onto two
    /// formulas, and a failed command counting as time saved.
    #[test]
    fn time_saved_counts_completed_commands_and_todays_words_only() {
        let history = History::open_in_memory().unwrap();
        let now = day(2026, 3, 10);
        let today = now.timestamp();
        let last_week = day(2026, 3, 3).timestamp();
        let keep = crate::history::Retention::entries(50);
        let words = "one two three four five six seven eight nine ten";
        history
            .insert(today, words, words, None, 0, Some(1), None, keep)
            .unwrap();
        history
            .insert_run(today, "pause", "Paused.", true, None, 5, 2, keep)
            .unwrap();
        history
            .insert_run(today, "email", "no gmail", false, None, 5, 3, keep)
            .unwrap();
        history
            .insert_run(last_week, "open", "Opened.", true, None, 5, 4, keep)
            .unwrap();

        let mut stats = Stats::default();
        stats.record_session(500, Stats::estimated_spoken_seconds(500), day(2026, 3, 3));
        let view = time_saved(&stats, &history, now).unwrap();

        assert_eq!(view.today.actions, 1, "the failed command saved nothing");
        assert_eq!(view.all_time.actions, 2);
        // Ten words: 15 s to type, 4 s to speak.
        assert!((view.today.dictation_seconds - 11.0).abs() < 0.1);
        assert_eq!(view.all_time.dictation_seconds, stats.time_saved_seconds());
    }

    #[test]
    fn empty_session_is_ignored() {
        let mut stats = Stats::default();
        stats.record_session(0, 5.0, day(2026, 3, 1));
        assert_eq!(stats, Stats::default(), "silence must not count");
    }

    /// Guards: stats were rebuilt as `default()` on every launch, zeroing the
    /// lifetime streak.
    #[test]
    fn stats_survive_a_round_trip_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.json");

        let mut stats = Stats::default();
        stats.record_session(120, 48.0, day(2026, 3, 1));
        stats.record_session(80, 32.0, day(2026, 3, 2));
        stats.save_to(&path).unwrap();

        let reloaded = Stats::load_from(&path);
        assert_eq!(reloaded, stats);
        assert_eq!(reloaded.total_words, 200);
        assert_eq!(reloaded.current_streak_days, 2);
    }

    #[test]
    fn missing_or_corrupt_stats_file_reads_as_zero() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            Stats::load_from(&dir.path().join("absent.json")),
            Stats::default()
        );

        let corrupt = dir.path().join("corrupt.json");
        fs::write(&corrupt, "{not json").unwrap();
        assert_eq!(Stats::load_from(&corrupt), Stats::default());
    }

    #[test]
    fn counts_words() {
        assert_eq!(Stats::word_count("hello there  world "), 3);
        assert_eq!(Stats::word_count(""), 0);
    }
}
