//! Exercises the real clock and the real `caffeinate`, which unit tests fake.
//!
//!   cargo run -p aloud-focus --example focus            # keep-awake only
//!   cargo run -p aloud-focus --example focus -- timer   # + a real 1-minute focus phase
//!
//! Keep-awake is checked against `pmset -g assertions`, the OS's own ledger.

use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aloud_focus::{island, Awake, Timer, TimerMode, TimerPreset};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Our assertion is the one `caffeinate` holding on behalf of this process.
fn holding(pid: u32) -> bool {
    let out = Command::new("pmset")
        .args(["-g", "assertions"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).contains(&format!("on behalf of Process ID {pid}"))
}

fn main() {
    let me = std::process::id();
    let mut awake = Awake::default();
    awake.start(Some(2), now_ms()).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    println!(
        "awake started: state {:?}, pmset holds for us: {}",
        awake.state(),
        holding(me)
    );
    println!(
        "island: {:?}",
        island::awake(&awake.state(), now_ms()).map(|i| i.detail)
    );
    awake.stop();
    std::thread::sleep(Duration::from_millis(500));
    println!(
        "awake stopped: state {:?}, pmset holds for us: {}",
        awake.state(),
        holding(me)
    );

    if std::env::args().any(|a| a == "timer") {
        let mut timer = Timer::new();
        timer.start(
            TimerMode::Pomodoro,
            TimerPreset { focus: 1, rest: 1 },
            now_ms(),
        );
        let began = now_ms();
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let now = now_ms();
            if let Some(done) = timer.tick(now) {
                println!(
                    "focus finished after {} ms (wanted 60000): {done:?}",
                    now - began
                );
                println!(
                    "then: {:?}",
                    island::timer(&timer.state(), now).map(|i| i.detail)
                );
                break;
            }
            if (now - began) % 10_000 < 250 {
                println!("left: {}", island::clock(timer.state().value_ms(now), true));
            }
        }
    }
}
