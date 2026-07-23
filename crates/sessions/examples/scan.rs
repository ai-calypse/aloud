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
            let model = session.model.as_deref().unwrap_or(session.agent.label());
            println!(
                "{:<7} {:<5} {:>6}  {}  [{model}]",
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
            for sub in &session.subagents {
                let s = if sub.busy { "●" } else { "○" };
                let m = sub.model.as_deref().unwrap_or("");
                println!("           ▸ {s} {} [{m}]", sub.label);
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
