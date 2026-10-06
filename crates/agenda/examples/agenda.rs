//! Reads the real Calendar and Reminders (read-only), and times it.
//!
//!   cargo run -p aloud-agenda --example agenda              # titles masked
//!   cargo run -p aloud-agenda --example agenda -- --titles  # titles shown
//!   cargo run -p aloud-agenda --example agenda -- --probe-complete <reminder id>
//!
//! The first run may raise the macOS Automation prompt for Calendar and
//! Reminders; that is the grant the app asks for too.

use std::time::Instant;

use aloud_agenda as agenda;
use chrono::{Local, TimeZone};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let show = args.iter().any(|a| a == "--titles");
    let name = |t: &str| {
        if show {
            t.to_string()
        } else {
            format!("<{} chars>", t.chars().count())
        }
    };
    let clock = |ms: i64| {
        Local
            .timestamp_millis_opt(ms)
            .unwrap()
            .format("%a %H:%M")
            .to_string()
    };

    let started = Instant::now();
    match agenda::events(agenda::day_start(0), agenda::day_start(2)) {
        Ok(events) => {
            println!("{} events in {:?}", events.len(), started.elapsed());
            for e in &events {
                println!(
                    "  {} - {}  {}{}{}  [{}]",
                    clock(e.start),
                    clock(e.end),
                    name(&e.title),
                    if e.all_day { "  (all day)" } else { "" },
                    if e.join_url.is_some() {
                        "  (join link)"
                    } else {
                        ""
                    },
                    e.calendar,
                );
            }
            let now = Local::now().timestamp_millis();
            println!(
                "island: {:?}",
                agenda::island(&events, now).map(|i| (i.detail, i.priority))
            );
        }
        Err(e) => println!("events failed in {:?}: {e:?}", started.elapsed()),
    }

    let started = Instant::now();
    match agenda::reminders(agenda::day_start(1), false) {
        Ok(rs) => {
            println!(
                "{} reminders due by end of today, in {:?}",
                rs.len(),
                started.elapsed()
            );
            for r in &rs {
                println!(
                    "  {} {} {}",
                    clock(r.due),
                    if r.all_day { "all-day" } else { "timed" },
                    name(&r.title)
                );
            }
        }
        Err(e) => println!("reminders failed in {:?}: {e:?}", started.elapsed()),
    }

    // Completing writes to the account, so it only runs when asked, with an
    // id from the listing above.
    if let Some(at) = args.iter().position(|a| a == "--probe-complete") {
        if let Some(id) = args.get(at + 1) {
            println!("complete {id}: {:?}", agenda::complete_reminder(id));
        }
    }
}
