//! Scans real browsers for playing media — what the unit tests cannot.
//!
//!   cargo run -p aloud-media --example browser -- scan              # every running browser, timed
//!   cargo run -p aloud-media --example browser -- scan "Brave Verify"   # one app by name (Chromium dictionary)
//!   cargo run -p aloud-media --example browser -- scan Safari safari    # ...or a Safari-style one
//!   cargo run -p aloud-media --example browser -- watch "Brave Verify"  # once a second, full walk then targeted
//!   cargo run -p aloud-media --example browser -- icon "Brave Browser"   # the badge icon
//!   cargo run -p aloud-media --example browser -- control "Brave Verify" toggle|seek 30|speed 150
//!
//! Needs "Allow JavaScript from Apple Events" on in the browser; without it
//! this prints exactly the issue the notch would show.

use std::time::Instant;

use aloud_media::browser::{self, Cmd};
use aloud_media::now_playing;

fn main() {
    tracing_subscriber::fmt::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let app = args.get(1).map(String::as_str);
    let safari = args.get(2).map(String::as_str) == Some("safari") || app == Some("Safari");
    match args.first().map(String::as_str) {
        Some("scan") => {
            let t = Instant::now();
            let (hits, issues) = match app {
                Some(app) => match browser::scan_app(app, safari, None, &[]) {
                    Ok((hits, slow)) => (
                        hits,
                        slow.iter()
                            .map(|t| format!("{app}: tab {t} did not answer"))
                            .collect(),
                    ),
                    Err(kind) => (vec![], vec![format!("{app}: {kind:?}")]),
                },
                None => {
                    let (hits, issues) = browser::scan();
                    (
                        hits,
                        issues
                            .iter()
                            .map(|i| format!("{}: {:?}", i.app, i.kind))
                            .collect(),
                    )
                }
            };
            println!(
                "{} media tab(s), {} issue(s) in {:?}",
                hits.len(),
                issues.len(),
                t.elapsed()
            );
            issues.iter().for_each(|i| println!("  issue: {i}"));
            hits.iter().for_each(|h| println!("{h:#?}"));
            if let Some(now) = hits.first() {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let art = rt.block_on(aloud_media::artwork(now));
                println!(
                    "artwork: {}",
                    art.map_or("none".into(), |u| format!("{} bytes of data: URI", u.len()))
                );
            }
        }
        Some("icon") => {
            let t = Instant::now();
            let icon = now_playing::app_icon(app.expect("icon <app>"));
            println!(
                "{:?} in {:?}",
                icon.map(|i| format!("{} bytes", i.len())),
                t.elapsed()
            );
        }
        Some("watch") => {
            let app = app.expect("watch <app>");
            let (mut only, mut skip): (Option<Vec<String>>, Vec<String>) = (None, vec![]);
            for i in 0..10 {
                let t = Instant::now();
                let (hits, slow) =
                    browser::scan_app(app, safari, only.as_deref(), &skip).unwrap_or_default();
                println!(
                    "#{i} {} ({}) -> {} hit(s), {} newly unanswering, in {:?}",
                    if only.is_some() {
                        "targeted"
                    } else {
                        "full walk"
                    },
                    hits.iter()
                        .map(|n| format!("{:?} {}ms", n.state, n.position_ms))
                        .collect::<Vec<_>>()
                        .join(", "),
                    hits.len(),
                    slow.len(),
                    t.elapsed()
                );
                skip.extend(slow);
                // Mirrors the crate: a walk, then targeted reads, then a walk.
                only = if only.is_none() && !hits.is_empty() && i % 3 != 2 {
                    Some(hits.iter().filter_map(|n| n.source.tab.clone()).collect())
                } else {
                    None
                };
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        Some("control") => {
            let (hits, _) = match browser::scan_app(app.expect("control <app>"), safari, None, &[])
            {
                Ok((h, _)) => (h, ()),
                Err(e) => return println!("cannot scan: {e:?}"),
            };
            let Some(now) = hits.first() else {
                return println!("no media tab to control");
            };
            let n: f64 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.0);
            let cmd = match args.get(2).map(String::as_str) {
                Some("play") => Cmd::Play,
                Some("pause") => Cmd::Pause,
                Some("seek") => Cmd::SeekSeconds(n),
                Some("speed") => Cmd::Speed(n / 100.0),
                _ => Cmd::Toggle,
            };
            println!(
                "before: {:?} at {}ms, {}%",
                now.state, now.position_ms, now.speed_pct
            );
            println!("control: {:?}", browser::control(&now.source, cmd));
            std::thread::sleep(std::time::Duration::from_millis(600));
            let after = browser::scan_app(app.unwrap(), safari, None, &[])
                .unwrap_or_default()
                .0;
            println!(
                "after:  {:?}",
                after.first().map(|n| (n.state, n.position_ms, n.speed_pct))
            );
        }
        _ => {
            let t = Instant::now();
            let reading = now_playing::read(true);
            println!("read(true) in {:?}\n{reading:#?}", t.elapsed());
        }
    }
}
