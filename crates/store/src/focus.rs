//! Finished focus time, for the streak and the seven-day chart.
//!
//! `focus.sqlite`, the `timings.sqlite` pattern: a small file of its own, so a
//! bad write can never touch history or settings. Each row stores the *local
//! day it happened on* as text, decided when it was written — a streak is a
//! run of calendar days as the user lived them, and re-deriving the day from a
//! timestamp later would move a session when they cross a timezone.

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate};
use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;

/// One day of the chart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FocusDay {
    /// `YYYY-MM-DD`, local.
    pub day: String,
    /// Finished sessions that day.
    pub sessions: u32,
    pub minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FocusStats {
    /// Consecutive days with a session, ending today — or yesterday, while
    /// today is still young.
    pub streak: u32,
    /// Seven days, oldest first, ending today; empty days included.
    pub week: Vec<FocusDay>,
}

pub struct FocusLog {
    conn: Connection,
}

impl FocusLog {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    fn migrate(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS focus_sessions (
                id      INTEGER PRIMARY KEY AUTOINCREMENT,
                day     TEXT    NOT NULL,
                kind    TEXT    NOT NULL,
                secs    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS focus_sessions_day ON focus_sessions (day);",
        )
        .context("creating focus schema")?;
        Ok(())
    }

    pub fn record(&self, day: NaiveDate, kind: &str, secs: u32) -> Result<()> {
        self.conn.execute(
            "INSERT INTO focus_sessions (day, kind, secs) VALUES (?1, ?2, ?3)",
            rusqlite::params![day.to_string(), kind, secs],
        )?;
        Ok(())
    }

    /// The streak and the week, as of `today`.
    pub fn stats(&self, today: NaiveDate) -> Result<FocusStats> {
        let mut stmt = self.conn.prepare(
            "SELECT day, COUNT(*), COALESCE(SUM(secs), 0) FROM focus_sessions
             GROUP BY day ORDER BY day DESC",
        )?;
        let rows: Vec<(NaiveDate, u32, u32)> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get(1)?, row.get::<_, u32>(2)?))
            })?
            .filter_map(Result::ok)
            .filter_map(|(day, n, secs)| Some((day.parse().ok()?, n, secs / 60)))
            .collect();
        let days: Vec<NaiveDate> = rows.iter().map(|(d, ..)| *d).collect();
        let week = (0..7)
            .rev()
            .map(|ago| {
                let day = today - Duration::days(ago);
                let (sessions, minutes) = rows
                    .iter()
                    .find(|(d, ..)| *d == day)
                    .map_or((0, 0), |(_, n, m)| (*n, *m));
                FocusDay {
                    day: day.to_string(),
                    sessions,
                    minutes,
                }
            })
            .collect();
        Ok(FocusStats {
            streak: streak(&days, today),
            week,
        })
    }
}

/// Consecutive days with a session, counting back from `today`. Today having
/// none yet does not break it — the day is not over — but yesterday having
/// none does. `days` need not be sorted or unique.
pub fn streak(days: &[NaiveDate], today: NaiveDate) -> u32 {
    let has = |d: NaiveDate| days.contains(&d);
    let mut day = if has(today) {
        today
    } else {
        today - Duration::days(1)
    };
    let mut run = 0;
    while has(day) {
        run += 1;
        day -= Duration::days(1);
    }
    run
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn a_streak_survives_a_young_today_but_not_a_missed_yesterday() {
        let days = [d("2026-10-02"), d("2026-10-01"), d("2026-09-30")];
        assert_eq!(streak(&days, d("2026-10-02")), 3);
        // Nothing yet today: still 3, the day is not over.
        assert_eq!(streak(&days, d("2026-10-03")), 3);
        // Yesterday missed as well: gone.
        assert_eq!(streak(&days, d("2026-10-04")), 0);
    }

    #[test]
    fn a_gap_ends_the_run_and_month_ends_are_just_days() {
        let days = [
            d("2026-10-02"),
            d("2026-10-01"),
            d("2026-09-29"),
            d("2026-09-28"),
        ];
        assert_eq!(streak(&days, d("2026-10-02")), 2);
        let across = [d("2026-03-01"), d("2026-02-28")];
        assert_eq!(streak(&across, d("2026-03-01")), 2);
    }

    #[test]
    fn the_week_is_seven_days_oldest_first_with_the_empty_ones_in() {
        let log = FocusLog::open_in_memory().unwrap();
        log.record(d("2026-10-04"), "pomodoro", 25 * 60).unwrap();
        log.record(d("2026-10-04"), "pomodoro", 25 * 60).unwrap();
        log.record(d("2026-10-02"), "stopwatch", 10 * 60).unwrap();
        // Outside the window: counts for the streak maths, not the chart.
        log.record(d("2026-09-20"), "pomodoro", 25 * 60).unwrap();

        let stats = log.stats(d("2026-10-04")).unwrap();
        assert_eq!(stats.week.len(), 7);
        assert_eq!(stats.week[0].day, "2026-09-28");
        assert_eq!(
            stats.week[6],
            FocusDay {
                day: "2026-10-04".into(),
                sessions: 2,
                minutes: 50
            }
        );
        assert_eq!(stats.week[4].minutes, 10);
        assert_eq!(stats.week[5].sessions, 0);
        assert_eq!(
            stats.streak, 1,
            "the 3rd is empty, so the 2nd's run is cut off from today"
        );
    }
}
