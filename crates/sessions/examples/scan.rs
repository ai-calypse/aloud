//! One discovery cycle against the machine's real agents, printed.
//!
//! ```sh
//! cargo run -p aloud-sessions --example scan          # one poll
//! cargo run -p aloud-sessions --example scan -- watch # keep polling
//! ```
//!
//! Unit tests cannot prove this: the whole crate is `ps`, `lsof` and files
//! another process is writing. Run it three times to see the states —
//! mid-turn, a minute after the turn ends, and after quitting the agent
//! (gone within two polls).

use std::time::Duration;

fn main() {
    let watch = std::env::args().any(|a| a == "watch");
    let mut store = aloud_sessions::SessionStore::default();

    loop {
        store.poll();
        let sessions = store.sessions();
        if sessions.is_empty() {
            println!("no agents running");
        }
        for session in &sessions {
            let state = if session.busy { "busy" } else { "idle" };
            println!(
                "{:<7} {:<5} {:>6}  {}",
                session.agent.label(),
                state,
                session.pid,
                session.label()
            );
            if let Some(tool) = &session.running {
                match &tool.detail {
                    Some(detail) => println!("           └ {}: {detail}", tool.name),
                    None => println!("           └ {}", tool.name),
                }
            }
            if session.transcript.is_none() {
                println!("           └ (no transcript found)");
            }
        }

        if !watch {
            return;
        }
        println!("---");
        // Matches the 3s poll the app will run; the grace window is counted in
        // polls, so watching at a different rate would not show the real timing.
        std::thread::sleep(Duration::from_secs(3));
    }
}
