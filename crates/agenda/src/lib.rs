//! Today's events and reminders, read through Calendar.app and Reminders.app
//! with JXA (`aloud_native::run_jxa`).
//!
//! JXA rather than EventKit: Calendar and Reminders are scripted under the
//! Automation permission the app already handles, while EventKit needs its own
//! entitlement and usage string — and from a script host it answered "write
//! only" and an empty list on the machine this was built on. The cost is
//! speed and a hole in the dictionary, both handled here:
//!
//! - **Recurring events** come back as their first occurrence only, so the
//!   rule is expanded in [`recur`].
//! - **Read-only calendars** (holidays, birthdays, Siri suggestions, other
//!   subscriptions) are skipped. Filtering a subscription's thousands of
//!   events took ~10s a calendar; the calendars you write to take ~0.1s.
//!   ponytail: a calendar shared read-only with you is skipped too; EventKit
//!   through a signed helper is the way to read those.
//!
//! Read-only on the user's account except [`complete_reminder`].

use std::collections::HashSet;

use aloud_islands::{Island, IslandAction, IslandKind, IslandPriority};
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};

pub mod recur;

/// Why a read failed, in the one distinction the view acts on: a missing
/// Automation grant has a button (System Settings), anything else is words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AgendaError {
    NotPermitted { app: String },
    Failed { message: String },
}

impl From<aloud_native::ScriptError> for AgendaError {
    fn from(e: aloud_native::ScriptError) -> Self {
        match e {
            aloud_native::ScriptError::NotPermitted { app } => Self::NotPermitted { app },
            other => Self::Failed {
                message: other.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// Unique per occurrence: a recurring event's uid is shared by all of them.
    pub id: String,
    pub title: String,
    /// Unix ms. An all-day event runs local midnight to the next.
    #[specta(type = specta_typescript::Number)]
    pub start: i64,
    #[specta(type = specta_typescript::Number)]
    pub end: i64,
    pub all_day: bool,
    pub location: Option<String>,
    /// A video-call link, from the event's URL or its location.
    pub join_url: Option<String>,
    pub calendar: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: String,
    pub title: String,
    #[specta(type = specta_typescript::Number)]
    pub due: i64,
    /// Due on a day, not at a time.
    pub all_day: bool,
    pub list: String,
}

/// One event as the script reports it, before recurrence is expanded.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    uid: String,
    title: Option<String>,
    start: String,
    end: String,
    all_day: bool,
    location: Option<String>,
    url: Option<String>,
    recurrence: Option<String>,
    #[serde(default)]
    excluded: Vec<String>,
    calendar: String,
}

const EVENTS_JXA: &str = r#"
const cal = Application('Calendar');
const from = new Date(ARG.from), to = new Date(ARG.to);
const out = [];
for (const c of cal.calendars()) {
  if (!c.writable() || ARG.skip.includes(c.name())) continue;
  const calendar = c.name();
  const evs = c.events.whose({_and: [
    {startDate: {_lessThan: to}},
    {_or: [{endDate: {_greaterThan: from}}, {recurrence: {_beginsWith: 'FREQ'}}]},
  ]})();
  for (const e of evs) {
    const recurrence = e.recurrence();
    out.push({
      uid: e.uid(), title: e.summary(),
      start: e.startDate().toISOString(), end: e.endDate().toISOString(),
      allDay: e.alldayEvent(), location: e.location(), url: e.url(),
      recurrence,
      excluded: recurrence ? (e.excludedDates() || []).map(d => d.toISOString()) : [],
      calendar,
    });
  }
}
return JSON.stringify(out);
"#;

/// The calendar Reminders mirrors its due dates into: its events are the
/// reminders the Reminders view already lists.
const SKIP_CALENDARS: [&str; 1] = ["Scheduled Reminders"];

fn local(iso: &str) -> Option<NaiveDateTime> {
    Some(
        DateTime::parse_from_rfc3339(iso)
            .ok()?
            .with_timezone(&Local)
            .naive_local(),
    )
}

fn ms(at: NaiveDateTime) -> Option<i64> {
    // A wall-clock time inside a spring-forward gap does not exist.
    Some(
        Local
            .from_local_datetime(&at)
            .earliest()?
            .timestamp_millis(),
    )
}

/// Events overlapping `[from_ms, to_ms)`, recurring ones expanded, all-day
/// first and then by start.
pub fn events(from_ms: i64, to_ms: i64) -> Result<Vec<Event>, AgendaError> {
    let arg = serde_json::json!({
        "from": DateTime::from_timestamp_millis(from_ms).map(|d| d.to_rfc3339()),
        "to": DateTime::from_timestamp_millis(to_ms).map(|d| d.to_rfc3339()),
        "skip": SKIP_CALENDARS,
    });
    let json = aloud_native::run_jxa("Calendar", EVENTS_JXA, &arg)?;
    let raw: Vec<Raw> = serde_json::from_str(&json).map_err(|e| AgendaError::Failed {
        message: format!("could not read Calendar: {e}"),
    })?;
    Ok(expand(&raw, from_ms, to_ms))
}

fn expand(raw: &[Raw], from_ms: i64, to_ms: i64) -> Vec<Event> {
    let mut out: Vec<Event> = Vec::new();
    let mut seen = HashSet::new();
    for r in raw {
        let (Some(start), Some(end)) = (local(&r.start), local(&r.end)) else {
            continue;
        };
        let len = end - start;
        let mut starts = vec![start];
        let rule = r.recurrence.as_deref().filter(|s| !s.is_empty());
        if let Some(rule) = rule.and_then(recur::Rule::parse) {
            let limit = DateTime::from_timestamp_millis(to_ms)
                .map_or(start, |d| d.with_timezone(&Local).naive_local());
            let excluded: HashSet<NaiveDate> = r
                .excluded
                .iter()
                .filter_map(|e| local(e))
                .map(|d| d.date())
                .collect();
            starts = rule
                .occurrences(start, limit)
                .into_iter()
                .filter(|s| !excluded.contains(&s.date()))
                .collect();
        }
        for s in starts {
            let (Some(s_ms), Some(e_ms)) = (ms(s), ms(s + len)) else {
                continue;
            };
            if s_ms >= to_ms || e_ms <= from_ms || !seen.insert((r.uid.clone(), s_ms)) {
                continue;
            }
            out.push(Event {
                id: format!("{}@{s_ms}", r.uid),
                title: r
                    .title
                    .clone()
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| "Untitled".into()),
                start: s_ms,
                end: e_ms,
                all_day: r.all_day,
                join_url: join_url(r.url.as_deref(), r.location.as_deref()),
                location: r.location.clone().filter(|l| !l.is_empty()),
                calendar: r.calendar.clone(),
            });
        }
    }
    out.sort_by_key(|e| (!e.all_day, e.start, e.title.clone()));
    out
}

const CALL_HOSTS: [&str; 6] = [
    "zoom.us",
    "meet.google.com",
    "teams.microsoft.com",
    "teams.live.com",
    "webex.com",
    "whereby.com",
];

/// A video-call link out of an event's URL field or its location text. Only
/// known call hosts over https: this becomes a "Join" button, and a button
/// must not open whatever an invite's text says.
pub fn join_url(url: Option<&str>, location: Option<&str>) -> Option<String> {
    let known = |candidate: &str| {
        let candidate = candidate.trim_matches(|c: char| ",.;)(<>\"'".contains(c));
        let rest = candidate.strip_prefix("https://")?;
        let host = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
        CALL_HOSTS
            .iter()
            .any(|h| host == *h || host.ends_with(&format!(".{h}")))
            .then(|| candidate.to_string())
    };
    url.and_then(known)
        .or_else(|| location?.split_whitespace().find_map(known))
}

/// The event worth a place beside the camera: one starting within an hour,
/// else one in progress. All-day events are the day, not the next thing.
pub fn next_event(events: &[Event], now: i64) -> Option<&Event> {
    const LEAD_MS: i64 = 60 * 60_000;
    let timed = || events.iter().filter(|e| !e.all_day && e.end > now);
    timed()
        .filter(|e| e.start > now && e.start - now <= LEAD_MS)
        .min_by_key(|e| e.start)
        .or_else(|| timed().filter(|e| e.start <= now).min_by_key(|e| e.end))
}

pub const NEXT_ISLAND_ID: &str = "calendar:next";

/// The next-event island, None when nothing is near. Minute resolution, so an
/// identical re-push each minute sends nothing until the words change.
pub fn island(events: &[Event], now: i64) -> Option<Island> {
    let event = next_event(events, now)?;
    let until = event.start - now;
    let (detail, priority) = if until <= 0 {
        ("now".to_string(), IslandPriority::Normal)
    } else {
        let mins = (until + 59_999) / 60_000;
        (
            format!("in {mins} min"),
            if mins <= 10 {
                IslandPriority::High
            } else {
                IslandPriority::Normal
            },
        )
    };
    Some(Island {
        id: NEXT_ISLAND_ID.into(),
        kind: IslandKind::Live,
        priority,
        icon: Some("calendar".into()),
        title: event.title.clone(),
        detail: Some(detail),
        actions: event
            .join_url
            .iter()
            .map(|url| IslandAction::new("Join", "calendar.join").arg("url", url))
            .collect(),
        expires: None,
    })
}

const REMINDERS_JXA: &str = r#"
const R = Application('Reminders');
const to = new Date(ARG.to);
const q = R.reminders.whose({_and: [{completed: false}, {dueDate: {_lessThan: to}}]});
const id = q.id(), name = q.name(), due = q.dueDate(), day = q.alldayDueDate();
const lists = q.container().map(c => c.name());
return JSON.stringify(id.map((_, i) => ({
  id: id[i], title: name[i], due: due[i].toISOString(),
  allDay: due[i].getTime() === day[i].getTime(), list: lists[i],
})));
"#;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReminder {
    id: String,
    title: Option<String>,
    due: String,
    all_day: bool,
    list: Option<String>,
}

/// Incomplete reminders due before `to_ms` — overdue ones included — soonest
/// first.
pub fn reminders(to_ms: i64) -> Result<Vec<Reminder>, AgendaError> {
    let arg =
        serde_json::json!({ "to": DateTime::from_timestamp_millis(to_ms).map(|d| d.to_rfc3339()) });
    let json = aloud_native::run_jxa("Reminders", REMINDERS_JXA, &arg)?;
    let raw: Vec<RawReminder> = serde_json::from_str(&json).map_err(|e| AgendaError::Failed {
        message: format!("could not read Reminders: {e}"),
    })?;
    Ok(parse_reminders(raw))
}

fn parse_reminders(raw: Vec<RawReminder>) -> Vec<Reminder> {
    let mut out: Vec<Reminder> = raw
        .into_iter()
        .filter_map(|r| {
            Some(Reminder {
                due: DateTime::parse_from_rfc3339(&r.due)
                    .ok()?
                    .timestamp_millis(),
                title: r
                    .title
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| "Untitled".into()),
                all_day: r.all_day,
                list: r.list.unwrap_or_default(),
                id: r.id,
            })
        })
        .collect();
    out.sort_by_key(|r| r.due);
    out
}

const COMPLETE_JXA: &str = r#"
const r = Application('Reminders').reminders.byId(ARG.id);
if (!r.exists()) throw new Error('that reminder is gone');
r.completed = true;
return 'ok';
"#;

/// Ticks one reminder off: the one thing here that writes to the user's
/// account. The id is checked before it reaches a script, though it travels
/// as data and is never interpolated.
pub fn complete_reminder(id: &str) -> Result<(), AgendaError> {
    if !id.starts_with("x-apple-reminder://") {
        return Err(AgendaError::Failed {
            message: "that is not a reminder".into(),
        });
    }
    aloud_native::run_jxa("Reminders", COMPLETE_JXA, &serde_json::json!({ "id": id }))?;
    Ok(())
}

/// Whether `app` (a process name, "Calendar") is running, without launching
/// it: scripting an app that is not running starts it.
pub fn app_running(app: &str) -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", app])
        .stdout(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Local midnight at the start of the day `days` from today, in unix ms.
pub fn day_start(days: i64) -> i64 {
    let day = Local::now().date_naive() + Duration::days(days);
    day.and_hms_opt(0, 0, 0).and_then(ms).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iso(s: &str) -> String {
        let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        Local
            .from_local_datetime(&naive)
            .earliest()
            .unwrap()
            .to_rfc3339()
    }
    fn at(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(&iso(s))
            .unwrap()
            .timestamp_millis()
    }

    fn raw(uid: &str, start: &str, end: &str, rule: Option<&str>) -> Raw {
        Raw {
            uid: uid.into(),
            title: Some(uid.into()),
            start: iso(start),
            end: iso(end),
            all_day: false,
            location: None,
            url: None,
            recurrence: rule.map(Into::into),
            excluded: vec![],
            calendar: "Work".into(),
        }
    }

    /// The reported bug: a recurring stand-up missing from "today".
    #[test]
    fn a_recurring_event_shows_on_the_day_it_falls_not_the_day_it_began() {
        let standup = raw(
            "standup",
            "2026-04-01 09:30",
            "2026-04-01 09:45",
            Some("FREQ=DAILY"),
        );
        let got = expand(&[standup], at("2026-10-04 00:00"), at("2026-10-05 00:00"));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].start, at("2026-10-04 09:30"));
        assert_eq!(got[0].end - got[0].start, 15 * 60_000);
    }

    #[test]
    fn an_excluded_occurrence_is_skipped() {
        let mut standup = raw(
            "standup",
            "2026-04-01 09:30",
            "2026-04-01 09:45",
            Some("FREQ=DAILY"),
        );
        standup.excluded = vec![iso("2026-10-04 09:30")];
        assert!(expand(&[standup], at("2026-10-04 00:00"), at("2026-10-05 00:00")).is_empty());
    }

    /// A rule this does not parse still shows the event once, rather than
    /// dropping it.
    #[test]
    fn an_unsupported_rule_falls_back_to_the_first_occurrence() {
        let odd = raw(
            "odd",
            "2026-10-04 10:00",
            "2026-10-04 11:00",
            Some("FREQ=MONTHLY;BYSETPOS=1;BYDAY=MO"),
        );
        assert_eq!(
            expand(&[odd], at("2026-10-04 00:00"), at("2026-10-05 00:00")).len(),
            1
        );
    }

    #[test]
    fn an_event_running_across_midnight_is_in_both_days_and_the_edges_are_open() {
        let late = raw("late", "2026-10-04 23:00", "2026-10-05 01:00", None);
        assert_eq!(
            expand(
                std::slice::from_ref(&late),
                at("2026-10-05 00:00"),
                at("2026-10-06 00:00")
            )
            .len(),
            1
        );
        let ends_at_midnight = raw("e", "2026-10-04 22:00", "2026-10-05 00:00", None);
        assert!(expand(
            &[ends_at_midnight],
            at("2026-10-05 00:00"),
            at("2026-10-06 00:00")
        )
        .is_empty());
    }

    #[test]
    fn all_day_events_sort_first() {
        let mut holiday = raw("h", "2026-10-04 00:00", "2026-10-04 23:59", None);
        holiday.all_day = true;
        let lunch = raw("lunch", "2026-10-04 12:00", "2026-10-04 13:00", None);
        let got = expand(
            &[lunch, holiday],
            at("2026-10-04 00:00"),
            at("2026-10-05 00:00"),
        );
        assert_eq!(
            got.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
            ["h", "lunch"]
        );
    }

    fn event(title: &str, start: i64, end: i64) -> Event {
        Event {
            id: title.into(),
            title: title.into(),
            start,
            end,
            all_day: false,
            location: None,
            join_url: None,
            calendar: "Work".into(),
        }
    }

    const MIN: i64 = 60_000;

    #[test]
    fn the_island_picks_what_starts_within_the_hour_before_what_is_running() {
        let now = 1_000 * MIN;
        let running = event("long meeting", now - 30 * MIN, now + 60 * MIN);
        let soon = event("design sync", now + 25 * MIN, now + 55 * MIN);
        let later = event("gym", now + 90 * MIN, now + 120 * MIN);
        let events = [running.clone(), soon, later.clone()];

        let i = island(&events, now).unwrap();
        assert_eq!(
            (i.title.as_str(), i.detail.as_deref()),
            ("design sync", Some("in 25 min"))
        );
        assert_eq!(i.priority, IslandPriority::Normal);

        // Nothing starting soon: the one in progress.
        let i = island(&[running, later], now).unwrap();
        assert_eq!(
            (i.title.as_str(), i.detail.as_deref()),
            ("long meeting", Some("now"))
        );

        assert!(island(&[event("gym", now + 90 * MIN, now + 120 * MIN)], now).is_none());
        assert!(island(&[event("done", now - 90 * MIN, now - 30 * MIN)], now).is_none());
    }

    #[test]
    fn minutes_round_up_and_the_last_ten_are_high_priority() {
        let now = 0;
        let i = island(&[event("a", 9 * MIN + 1, 30 * MIN)], now).unwrap();
        assert_eq!(i.detail.as_deref(), Some("in 10 min"));
        assert_eq!(i.priority, IslandPriority::High);
        let i = island(&[event("a", 1, 30 * MIN)], now).unwrap();
        assert_eq!(i.detail.as_deref(), Some("in 1 min"));
    }

    #[test]
    fn all_day_events_are_not_the_next_thing() {
        let mut e = event("holiday", -MIN, 600 * MIN);
        e.all_day = true;
        assert!(island(&[e], 0).is_none());
    }

    /// "Join" opens a link, so only a known call host over https may become one.
    #[test]
    fn only_call_links_become_join() {
        assert_eq!(
            join_url(
                None,
                Some("Room 4 https://acme.zoom.us/j/123?pwd=x, passcode 9")
            ),
            Some("https://acme.zoom.us/j/123?pwd=x".into())
        );
        assert_eq!(
            join_url(Some("https://meet.google.com/abc-defg-hij"), None).as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
        assert_eq!(
            join_url(Some("http://zoom.us/j/1"), None),
            None,
            "not over https"
        );
        assert_eq!(join_url(Some("https://evil.example/zoom.us"), None), None);
        assert_eq!(join_url(Some("https://notzoom.us/j/1"), None), None);
        assert_eq!(join_url(Some("file:///etc/passwd"), Some("Room 4")), None);
    }

    #[test]
    fn reminders_sort_by_due_and_tell_all_day_from_timed() {
        let raw = |id: &str, due: &str, all_day| RawReminder {
            id: id.into(),
            title: Some(id.into()),
            due: due.into(),
            all_day,
            list: None,
        };
        let got = parse_reminders(vec![
            raw("b", "2026-10-04T04:00:00.000Z", true),
            raw("a", "2026-09-28T16:00:00.000Z", false),
            raw("bad", "yesterday", false),
        ]);
        assert_eq!(
            got.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert!(got[1].all_day && !got[0].all_day);
    }

    #[test]
    fn completing_refuses_anything_that_is_not_a_reminder_id() {
        assert!(complete_reminder("'; do shell script").is_err());
        assert!(complete_reminder("").is_err());
    }
}
