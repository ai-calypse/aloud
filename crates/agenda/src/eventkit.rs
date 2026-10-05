//! Reminders through EventKit, in process: about ten milliseconds where the
//! scripting bridge answers in a second (every property read on Reminders.app
//! costs ~100-200ms there, whatever it returns).
//!
//! Needs macOS 14's full-access grant. The older `requestAccessToEntityType:`
//! only ever yields write-only, which reads as an empty list: that is what the
//! first attempt at EventKit met. Asking needs
//! `NSRemindersFullAccessUsageDescription` in the running binary's Info.plist
//! (Tauri embeds `src-tauri/Info.plist` into the dev binary too); without it
//! macOS kills the process, so [`request_access`] checks for the key first.

use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2::{msg_send, sel};
use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventStore, EKReminder};
use objc2_foundation::{NSArray, NSBundle, NSDate, NSError, NSString};

use crate::Reminder;

/// What EventKit will do for Reminders right now, without asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Granted,
    /// Never asked: the one state a prompt can still change.
    Undecided,
    /// Denied, restricted, write-only, or an OS that predates the full-access
    /// call. The scripting path answers instead.
    Unavailable,
}

const USAGE_KEY: &str = "NSRemindersFullAccessUsageDescription";

fn store() -> Retained<EKEventStore> {
    // SAFETY: a plain `new` on a class with a designated initialiser.
    unsafe { EKEventStore::new() }
}

pub fn access() -> Access {
    // SAFETY: class method, no arguments to get wrong.
    let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Reminder) };
    match status {
        EKAuthorizationStatus::FullAccess => Access::Granted,
        EKAuthorizationStatus::NotDetermined if can_ask() => Access::Undecided,
        _ => Access::Unavailable,
    }
}

/// The OS has the full-access call and this binary carries the usage string.
fn can_ask() -> bool {
    let has_key = NSBundle::mainBundle()
        .objectForInfoDictionaryKey(&NSString::from_str(USAGE_KEY))
        .is_some();
    let sel = sel!(requestFullAccessToRemindersWithCompletion:);
    // SAFETY: respondsToSelector: is defined on every NSObject.
    let has_call: bool = unsafe { msg_send![&*store(), respondsToSelector: sel] };
    has_key && has_call
}

/// Raises the macOS prompt (once; the answer is remembered) and waits for it.
/// `false` on a refusal, a failure, or no answer within `wait`.
pub fn request_access(wait: Duration) -> bool {
    if !can_ask() {
        return false;
    }
    let (tx, rx) = mpsc::sync_channel(1);
    let handler = RcBlock::new(move |granted: Bool, _err: *mut NSError| {
        let _ = tx.try_send(granted.as_bool());
    });
    let store = store();
    // SAFETY: `handler` is a live block for the call, and the store copies it.
    unsafe { store.requestFullAccessToRemindersWithCompletion(&*handler as *const _ as *mut _) };
    rx.recv_timeout(wait).unwrap_or(false)
}

/// Incomplete reminders due before `to_ms`, overdue ones included, soonest
/// first — what the scripting read returns, from the same grant-checked store.
pub fn reminders(to_ms: i64) -> Option<Vec<Reminder>> {
    let store = store();
    let to = NSDate::dateWithTimeIntervalSince1970(to_ms as f64 / 1000.0);
    // SAFETY: both dates outlive the call; nil start means "no lower bound".
    let predicate = unsafe {
        store.predicateForIncompleteRemindersWithDueDateStarting_ending_calendars(
            None,
            Some(&to),
            None,
        )
    };
    let (tx, rx) = mpsc::sync_channel(1);
    let done = RcBlock::new(move |found: *mut NSArray<EKReminder>| {
        // SAFETY: EventKit hands a valid array, or nil, for the block's duration.
        let list = unsafe { found.as_ref() }
            .map(|a| a.iter().filter_map(|r| convert(&r)).collect::<Vec<_>>());
        let _ = tx.try_send(list);
    });
    // SAFETY: `done` is a live block for the call; the fetch retains it.
    let _fetch = unsafe { store.fetchRemindersMatchingPredicate_completion(&predicate, &done) };
    // ponytail: a stuck store returns None after 10s and the caller falls back to scripting.
    let mut out = rx.recv_timeout(Duration::from_secs(10)).ok()??;
    out.sort_by_key(|r| r.due);
    Some(out)
}

fn convert(r: &EKReminder) -> Option<Reminder> {
    // SAFETY: plain property reads on a reminder EventKit just returned.
    unsafe {
        let parts = r.dueDateComponents()?;
        let due = parts.date()?.timeIntervalSince1970();
        let title = r.title().to_string();
        Some(Reminder {
            id: format!("x-apple-reminder://{}", r.calendarItemIdentifier()),
            title: if title.is_empty() {
                "Untitled".into()
            } else {
                title
            },
            due: (due * 1000.0).round() as i64,
            // A due day with no time of day is all-day.
            all_day: parts.hour() == isize::MAX,
            list: r
                .calendar()
                .map(|c| c.title().to_string())
                .unwrap_or_default(),
        })
    }
}
