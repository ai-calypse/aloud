//! Times each way of reading (read-only): a bare `osascript`, the old JXA that
//! asked for each reminder's list one Apple event at a time, the one that ships
//! now, EventKit when it is already granted (never asks), and one day's events.
//!
//!   cargo run -p aloud-agenda --example reminders_timing

use std::time::Instant;

const OLD_PER_REMINDER_CONTAINER: &str = r#"
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

fn timed<T>(label: &str, f: impl FnOnce() -> T, show: impl FnOnce(&T) -> String) {
    let t = Instant::now();
    let out = f();
    println!("{label:<36} {:>8.1?}  {}", t.elapsed(), show(&out));
}

fn main() {
    let to = aloud_agenda::day_start(1);
    let arg = serde_json::json!({
        "to": chrono::DateTime::from_timestamp_millis(to).map(|d| d.to_rfc3339())
    });
    let n = |r: &Result<Vec<aloud_agenda::Reminder>, _>| match r {
        Ok(v) => format!("{} reminders", v.len()),
        Err(e) => format!("{e:?}"),
    };
    timed(
        "bare osascript (cold)",
        || aloud_native::run_jxa("Reminders", "return 'ok';", &arg),
        |r| format!("{r:?}"),
    );
    timed(
        "bare osascript (warm)",
        || aloud_native::run_jxa("Reminders", "return 'ok';", &arg),
        |r| format!("{r:?}"),
    );
    for i in 0..3 {
        timed(
            &format!("old: container per reminder #{i}"),
            || aloud_native::run_jxa("Reminders", OLD_PER_REMINDER_CONTAINER, &arg),
            |r| format!("{} bytes", r.as_ref().map_or(0, String::len)),
        );
    }
    for i in 0..3 {
        timed(
            &format!("new: scripted #{i}"),
            || aloud_agenda::reminders_scripted(to),
            n,
        );
    }
    #[cfg(target_os = "macos")]
    {
        let access = aloud_agenda::eventkit::access();
        println!("eventkit access: {access:?}");
        if access == aloud_agenda::eventkit::Access::Granted {
            for i in 0..3 {
                timed(
                    &format!("eventkit #{i}"),
                    || aloud_agenda::eventkit::reminders(to).ok_or("none"),
                    |r| format!("{:?}", r.as_ref().map(Vec::len)),
                );
            }
        }
    }
    let store =
        aloud_agenda::cache::Store::new(std::env::temp_dir().join("aloud-agenda-timing.json"));
    if let Ok(v) = aloud_agenda::reminders_scripted(to) {
        store.put(v);
    }
    timed(
        "cache hit (disk, new process)",
        || {
            aloud_agenda::cache::Store::new(std::env::temp_dir().join("aloud-agenda-timing.json"))
                .get()
        },
        |r| format!("{:?}", r.as_ref().map(Vec::len)),
    );
    timed(
        "cache hit (memory)",
        || store.get(),
        |r| format!("{:?}", r.as_ref().map(Vec::len)),
    );
    let _ = std::fs::remove_file(std::env::temp_dir().join("aloud-agenda-timing.json"));

    for (label, day) in [("today", 0), ("a week out", 7)] {
        let at = aloud_agenda::day_start(day) + 12 * 3_600_000;
        timed(
            &format!("events_on({label})"),
            || aloud_agenda::events_on(at),
            |r| format!("{:?}", r.as_ref().map(Vec::len)),
        );
    }
}
