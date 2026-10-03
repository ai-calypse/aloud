//! When a reminder is due, parsed strictly from what the model sends.
//!
//! The accepted shapes follow FradSer/mcp-server-apple-events'
//! `reminderDateParser` (MIT): a date, a local date and time, or ISO-8601 with
//! an offset. Anything else is refused rather than guessed, and a date that
//! would roll over (February 30th) is refused too — their check, and chrono's.

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, Timelike};

/// A due date in local time; `time` is None for an all-day reminder.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Due {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub time: Option<(u32, u32)>,
}

pub(crate) fn due_parts(text: &str) -> Result<Due, String> {
    let text = text.trim();
    let bad = || format!("couldn't read the due date {text:?}; use YYYY-MM-DD or YYYY-MM-DDTHH:MM");

    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Ok(Due {
            year: date.year(),
            month: date.month(),
            day: date.day(),
            time: None,
        });
    }
    let local = [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
    .or_else(|| {
        DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|at| at.with_timezone(&Local).naive_local())
    })
    .ok_or_else(bad)?;
    Ok(Due {
        year: local.year(),
        month: local.month(),
        day: local.day(),
        time: Some((local.hour(), local.minute())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_local_times_read_as_given() {
        assert_eq!(
            due_parts("2026-10-03T17:00").unwrap(),
            Due {
                year: 2026,
                month: 10,
                day: 3,
                time: Some((17, 0))
            }
        );
        assert_eq!(
            due_parts("2026-10-04 09:30:00").unwrap().time,
            Some((9, 30))
        );
        assert_eq!(due_parts("2026-12-25").unwrap().time, None);
    }

    /// AppleScript dates built field by field roll Feb 30 into March; a
    /// reminder on the wrong day is worse than an error the model can fix.
    #[test]
    fn impossible_and_vague_dates_are_refused() {
        assert!(due_parts("2026-02-30").is_err());
        assert!(due_parts("2026-13-01T10:00").is_err());
        assert!(due_parts("tomorrow at 5").is_err());
    }

    #[test]
    fn an_offset_is_converted_to_local_time() {
        let due = due_parts("2026-10-03T17:00:00+00:00").unwrap();
        let expected = DateTime::parse_from_rfc3339("2026-10-03T17:00:00+00:00")
            .unwrap()
            .with_timezone(&Local);
        assert_eq!(due.time, Some((expected.hour(), expected.minute())));
    }
}
