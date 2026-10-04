//! Calendar.app's scripting dictionary hands back a recurring event once, as
//! its first occurrence, with the rule as an RFC 5545 string. A daily stand-up
//! asked for "today" would simply be missing, so the occurrences are worked out
//! here from the rule.
//!
//! Supported: DAILY, WEEKLY, MONTHLY and YEARLY with INTERVAL, COUNT, UNTIL,
//! BYDAY (with an ordinal: `3MO`, `-1FR`), BYMONTHDAY and BYMONTH. Anything
//! else (BYSETPOS, BYWEEKNO…) is refused rather than guessed, and the caller
//! falls back to showing the event once.
//!
//! Dates are local wall-clock: a 9:00 meeting stays at 9:00 across a daylight
//! saving change, as the calendar means it.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    freq: Freq,
    interval: u32,
    count: Option<u32>,
    /// Last day an occurrence may fall on.
    until: Option<NaiveDate>,
    /// (ordinal, weekday): `3MO` is (Some(3), Mon), plain `MO` is (None, Mon).
    by_day: Vec<(Option<i32>, Weekday)>,
    by_month_day: Vec<i32>,
    by_month: Vec<u32>,
}

fn weekday(code: &str) -> Option<Weekday> {
    Some(match code {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return None,
    })
}

impl Rule {
    /// None for a rule this does not understand.
    pub fn parse(rule: &str) -> Option<Rule> {
        let mut r = Rule {
            freq: Freq::Daily,
            interval: 1,
            count: None,
            until: None,
            by_day: Vec::new(),
            by_month_day: Vec::new(),
            by_month: Vec::new(),
        };
        let mut freq = None;
        for part in rule.trim_start_matches("RRULE:").split(';') {
            let (key, value) = part.split_once('=')?;
            match key {
                "FREQ" => {
                    freq = Some(match value {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        "YEARLY" => Freq::Yearly,
                        _ => return None,
                    })
                }
                "INTERVAL" => r.interval = value.parse::<u32>().ok().filter(|n| *n > 0)?,
                "COUNT" => r.count = Some(value.parse().ok()?),
                // `20261231T045959Z` or `20261231`: the day is what matters.
                "UNTIL" => r.until = NaiveDate::parse_from_str(value.get(..8)?, "%Y%m%d").ok(),
                "BYDAY" => {
                    for token in value.split(',') {
                        let (num, code) = token.split_at(token.len().checked_sub(2)?);
                        let nth = if num.is_empty() {
                            None
                        } else {
                            Some(num.parse().ok()?)
                        };
                        r.by_day.push((nth, weekday(code)?));
                    }
                }
                "BYMONTHDAY" => {
                    for n in value.split(',') {
                        r.by_month_day.push(n.parse().ok()?);
                    }
                }
                "BYMONTH" => {
                    for n in value.split(',') {
                        r.by_month.push(n.parse().ok()?);
                    }
                }
                "WKST" => {}
                _ => return None,
            }
        }
        r.freq = freq?;
        Some(r)
    }

    /// Every occurrence at or after `start` and on or before `limit`, in
    /// order, with `start`'s time of day. COUNT is counted from `start`, so
    /// the walk cannot begin at the window.
    pub fn occurrences(&self, start: NaiveDateTime, limit: NaiveDateTime) -> Vec<NaiveDateTime> {
        let time = start.time();
        let mut out = Vec::new();
        // The shortest a period can be, so a period that starts past the limit
        // ends the walk even when its filters left it no days to look at.
        let span = match self.freq {
            Freq::Daily => 1,
            Freq::Weekly => 7,
            Freq::Monthly => 28,
            Freq::Yearly => 365,
        };
        let reach = (limit.date() - start.date()).num_days() + 7;
        for k in 0..10_000u32 {
            if i64::from(self.interval) * i64::from(k) * span > reach {
                break;
            }
            let Some(days) = self.period(start.date(), k) else {
                break;
            };
            for day in days.into_iter().filter(|d| *d >= start.date()) {
                let at = day.and_time(time);
                if at > limit
                    || self.until.is_some_and(|u| day > u)
                    || self.count.is_some_and(|c| out.len() as u32 >= c)
                {
                    return out;
                }
                out.push(at);
            }
        }
        out
    }

    /// The candidate days of the `k`th period, sorted. None when the period
    /// is past anything representable.
    fn period(&self, start: NaiveDate, k: u32) -> Option<Vec<NaiveDate>> {
        let step = i64::from(self.interval) * i64::from(k);
        let mut days = match self.freq {
            Freq::Daily => {
                let day = start.checked_add_signed(Duration::days(step))?;
                let ok = (self.by_day.is_empty()
                    || self.by_day.iter().any(|(_, w)| *w == day.weekday()))
                    && (self.by_month.is_empty() || self.by_month.contains(&day.month()));
                if ok {
                    vec![day]
                } else {
                    vec![]
                }
            }
            Freq::Weekly => {
                let monday =
                    start - Duration::days(i64::from(start.weekday().num_days_from_monday()));
                let base = monday.checked_add_signed(Duration::weeks(step))?;
                let wanted: Vec<Weekday> = if self.by_day.is_empty() {
                    vec![start.weekday()]
                } else {
                    self.by_day.iter().map(|(_, w)| *w).collect()
                };
                wanted
                    .into_iter()
                    .map(|w| base + Duration::days(i64::from(w.num_days_from_monday())))
                    .collect()
            }
            Freq::Monthly => {
                let months = i64::from(start.month0()) + step;
                let (year, month) = (
                    start.year() + (months.div_euclid(12)) as i32,
                    months.rem_euclid(12) as u32 + 1,
                );
                if !self.by_month.is_empty() && !self.by_month.contains(&month) {
                    vec![]
                } else {
                    self.days_in_month(start, year, month)
                }
            }
            Freq::Yearly => {
                let year = start.year().checked_add(step as i32)?;
                let months = if self.by_month.is_empty() {
                    vec![start.month()]
                } else {
                    self.by_month.clone()
                };
                months
                    .into_iter()
                    .flat_map(|m| self.days_in_month(start, year, m))
                    .collect()
            }
        };
        days.sort();
        Some(days)
    }

    /// The days of one month the BYDAY / BYMONTHDAY terms pick; with neither,
    /// the start's own day of the month (skipping a month that lacks it).
    fn days_in_month(&self, start: NaiveDate, year: i32, month: u32) -> Vec<NaiveDate> {
        let last = last_day(year, month);
        let mut days = Vec::new();
        if !self.by_month_day.is_empty() {
            for n in &self.by_month_day {
                let day = if *n > 0 {
                    *n as u32
                } else {
                    (last as i32 + 1 + n).max(0) as u32
                };
                days.extend(NaiveDate::from_ymd_opt(year, month, day));
            }
        } else if !self.by_day.is_empty() {
            for (nth, weekday) in &self.by_day {
                let matching: Vec<NaiveDate> = (1..=last)
                    .filter_map(|d| NaiveDate::from_ymd_opt(year, month, d))
                    .filter(|d| d.weekday() == *weekday)
                    .collect();
                match nth {
                    None => days.extend(matching),
                    Some(n) if *n > 0 => days.extend(matching.get(*n as usize - 1)),
                    Some(n) => days.extend(matching.iter().rev().nth((-n) as usize - 1)),
                }
            }
        } else {
            days.extend(NaiveDate::from_ymd_opt(year, month, start.day()));
        }
        days
    }
}

fn last_day(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next.0, next.1, 1)
        .and_then(|d| d.pred_opt())
        .map_or(28, |d| d.day())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn days(rule: &str, start: &str, limit: &str) -> Vec<String> {
        Rule::parse(rule)
            .unwrap()
            .occurrences(at(start), at(limit))
            .iter()
            .map(|d| d.format("%m-%d %H:%M").to_string())
            .collect()
    }

    /// The reason this file exists: a daily stand-up that began last spring.
    #[test]
    fn a_weekday_standup_from_months_ago_lands_today() {
        let got = days(
            "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO,TU,WE,TH,FR",
            "2026-04-01 09:30",
            "2026-10-06 23:59",
        );
        // 2026-10-04 is a Sunday: Friday the 2nd, then Monday the 5th and Tuesday the 6th.
        assert_eq!(
            &got[got.len() - 3..],
            ["10-02 09:30", "10-05 09:30", "10-06 09:30"]
        );
        assert!(!got.contains(&"10-03 09:30".to_string()));
    }

    #[test]
    fn a_fortnightly_meeting_skips_the_off_week() {
        let got = days(
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=TH",
            "2026-09-10 14:00",
            "2026-10-31 23:59",
        );
        assert_eq!(
            got,
            ["09-10 14:00", "09-24 14:00", "10-08 14:00", "10-22 14:00"]
        );
    }

    /// COUNT runs from the first occurrence, not from the window: the fifth
    /// Presidents' Day is the last, whichever year you look in.
    #[test]
    fn count_is_counted_from_the_start() {
        let got = days(
            "FREQ=YEARLY;INTERVAL=1;COUNT=5;BYMONTH=2;BYDAY=3MO",
            "2025-02-17 00:00",
            "2040-01-01 00:00",
        );
        assert_eq!(got.len(), 5);
        assert_eq!(got[1], "02-16 00:00", "third Monday of February 2026");
    }

    #[test]
    fn until_ends_the_series_on_that_day() {
        let got = days(
            "FREQ=DAILY;UNTIL=20261003T035959Z",
            "2026-10-01 08:00",
            "2026-10-10 00:00",
        );
        assert_eq!(got, ["10-01 08:00", "10-02 08:00", "10-03 08:00"]);
    }

    #[test]
    fn monthly_ordinals_and_the_last_friday() {
        let got = days(
            "FREQ=MONTHLY;BYDAY=-1FR",
            "2026-08-28 16:00",
            "2026-11-30 00:00",
        );
        assert_eq!(
            got,
            ["08-28 16:00", "09-25 16:00", "10-30 16:00", "11-27 16:00"]
        );
        let got = days(
            "FREQ=MONTHLY;BYMONTHDAY=31",
            "2026-01-31 10:00",
            "2026-05-01 00:00",
        );
        assert_eq!(
            got,
            ["01-31 10:00", "03-31 10:00"],
            "months without a 31st are skipped"
        );
    }

    #[test]
    fn what_it_does_not_understand_is_refused_not_guessed() {
        assert!(Rule::parse("FREQ=MONTHLY;BYDAY=MO;BYSETPOS=1").is_none());
        assert!(Rule::parse("FREQ=HOURLY").is_none());
        assert!(Rule::parse("INTERVAL=2").is_none());
        assert!(Rule::parse("nonsense").is_none());
    }

    /// A rule that can never match must end, not hang the notch.
    #[test]
    fn an_impossible_rule_terminates() {
        let got = days(
            "FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=30",
            "2026-02-27 10:00",
            "2090-01-01 00:00",
        );
        assert!(got.is_empty());
    }
}
