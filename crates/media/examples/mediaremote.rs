//! Runs the real helper on this Mac, which the unit tests cannot:
//!
//!   cargo run -p aloud-media --example mediaremote -- watch [seconds]   # what is playing, twice a second
//!   cargo run -p aloud-media --example mediaremote -- send play|pause|toggle|next|previous
//!   cargo run -p aloud-media --example mediaremote -- seek 90           # to second 90
//!   cargo run -p aloud-media --example mediaremote -- restart           # kill the child, watch it come back
//!   cargo run -p aloud-media --example mediaremote -- read              # `now_playing::read`, the source choice
//!
//! Needs `crates/media/mediaremote/build.sh` to have run. It only reads what is
//! playing; `send` and `seek` act on whatever macOS calls the now-playing app.

use std::time::{Duration, Instant};

use aloud_media::mediaremote;
use aloud_media::now_playing;
use aloud_media::Transport;

fn show() {
    match mediaremote::current() {
        Some(n) => println!(
            "{:?} {} - {} [{}] {}/{} ms, {}%, art={:?}, bundle={:?}",
            n.state,
            n.artist,
            n.title,
            n.source.app,
            n.position_ms,
            n.duration_ms,
            n.speed_pct,
            n.artwork_url,
            n.source.bundle_id,
        ),
        None => println!("nothing (working={})", mediaremote::working()),
    }
}

fn wait_for_frame() {
    let t = Instant::now();
    while !mediaremote::working() && t.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("helper up after {:?}", t.elapsed());
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    mediaremote::start(|| println!("  (changed)"));
    mediaremote::set_enabled(true);
    wait_for_frame();
    match args.first().map(String::as_str) {
        Some("watch") => {
            let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
            for _ in 0..secs * 2 {
                show();
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        Some("send") => {
            let t = match args.get(1).map(String::as_str) {
                Some("play") => Transport::Play,
                Some("pause") => Transport::Pause,
                Some("next") => Transport::Next,
                Some("previous") => Transport::Previous,
                _ => Transport::Toggle,
            };
            show();
            println!("send {t:?}: {:?}", mediaremote::send(t));
            std::thread::sleep(Duration::from_millis(600));
            show();
        }
        Some("seek") => {
            let secs: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            println!("seek: {:?}", mediaremote::seek(secs * 1000));
            std::thread::sleep(Duration::from_millis(600));
            show();
        }
        Some("restart") => {
            show();
            println!("killing the stream child");
            let _ = std::process::Command::new("pkill")
                .args(["-f", "mediaremote-adapter.pl .* stream"])
                .status();
            std::thread::sleep(Duration::from_millis(300));
            println!("working right after the kill: {}", mediaremote::working());
            wait_for_frame();
            show();
            println!("disabling: the helper must be gone");
            mediaremote::set_enabled(false);
            std::thread::sleep(Duration::from_millis(500));
            let left = std::process::Command::new("pgrep")
                .args(["-f", "mediaremote-adapter.pl .* stream"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            println!("stream processes left: {:?}", left);
        }
        Some("read") => {
            let r = now_playing::read(true);
            println!("{:#?}", r.now);
            println!("browser issues: {:?}", r.browsers);
        }
        _ => println!("watch | send <cmd> | seek <secs> | restart | read"),
    }
    mediaremote::shutdown();
}
