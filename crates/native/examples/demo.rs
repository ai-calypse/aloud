//! Exercises the demo-path native tools against real apps.
//!
//! The catalogue's unit tests check schemas and flags; only this proves the
//! AppleScript actually drives the app. Run with the tool name, or with no
//! arguments for the read-only ones:
//!
//! ```sh
//! cargo run -p aloud-native --example demo               # the safe reads
//! cargo run -p aloud-native --example demo -- window left
//! cargo run -p aloud-native --example demo -- organize   # DRY RUN, prints the plan
//! cargo run -p aloud-native --example demo -- call ALOUD_NOTES_CREATE '{"title":"Cake","body":"- flour"}'
//! cargo run -p aloud-native --example demo -- fast "open calculator app"   # the no-model route
//! cargo run -p aloud-native --example demo -- contacts Suhani   # lookup timing, handles masked
//! cargo run -p aloud-native --example demo -- prepare ALOUD_MESSAGES_SEND '{"to":"Suhani","text":"test","dry_run":true}'
//! ```

use aloud_native::contacts::{self, Match};
use serde_json::json;

/// Contact output may land in logs, so all but the last two characters go.
fn mask(s: &str) -> String {
    let n = s.chars().count();
    s.chars()
        .enumerate()
        .map(|(i, c)| if i + 2 < n { '•' } else { c })
        .collect()
}

fn call(name: &str, args: serde_json::Value) {
    let started = std::time::Instant::now();
    match aloud_native::execute(name, &args) {
        Some(Ok(v)) => println!("✓ {name} ({:?})\n  {v}", started.elapsed()),
        Some(Err(e)) => println!("✗ {name} ({:?})\n  {e}", started.elapsed()),
        None => println!("? {name} is not a native tool"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => {
            // Read-only: safe to run repeatedly, changes nothing.
            call("ALOUD_CLIPBOARD", json!({ "action": "read" }));
            call("ALOUD_BROWSER_TAB", json!({ "action": "current" }));
        }
        [cmd, rest @ ..] if cmd == "window" => {
            let position = rest.first().map_or("left", String::as_str);
            call("ALOUD_WINDOW_ARRANGE", json!({ "position": position }));
        }
        [cmd] if cmd == "clipboard" => {
            call(
                "ALOUD_CLIPBOARD",
                json!({ "action": "write", "text": "written by Aloud" }),
            );
            call("ALOUD_CLIPBOARD", json!({ "action": "read" }));
        }
        [cmd] if cmd == "browser" => {
            call(
                "ALOUD_BROWSER_TAB",
                json!({ "action": "open", "url": "https://example.com" }),
            );
        }
        [cmd] if cmd == "organize" => {
            // Deliberately not run against a real folder here — the unit tests
            // cover apply/undo on a scratch directory, and this example must
            // never be the thing that shuffles someone's Downloads by accident.
            println!("organize is covered by `cargo test -p aloud-native organize`.");
            println!("To try it live, use the app — the gate previews every move first.");
        }
        // What the fast path makes of an utterance, then running it (web tasks only print).
        [cmd, said] if cmd == "fast" => {
            let started = std::time::Instant::now();
            let installed = aloud_selection::installed_apps();
            match aloud_native::fast::route(said, &installed) {
                Some(aloud_native::fast::Fast::Tool { name, args, done }) => {
                    println!("→ {name} {args}  ({:?} to route)", started.elapsed());
                    call(name, args);
                    println!("  says: {done}");
                }
                Some(aloud_native::fast::Fast::Web { query }) => {
                    println!("→ browser task (engine), query {query:?}")
                }
                None => println!("→ the brain"),
            }
        }
        [cmd, query] if cmd == "contacts" => {
            let started = std::time::Instant::now();
            let people = match contacts::find(query) {
                Ok(people) => people,
                Err(e) => return println!("✗ contacts ({:?})\n  {e}", started.elapsed()),
            };
            println!("{} match(es) in {:?}", people.len(), started.elapsed());
            for (i, p) in people.iter().enumerate() {
                let masked = |v: &[String]| v.iter().map(|s| mask(s)).collect::<Vec<_>>();
                println!(
                    "  #{} phones {:?} emails {:?}",
                    i + 1,
                    masked(&p.phones),
                    masked(&p.emails)
                );
            }
            match contacts::rank(query, people) {
                Match::One(_) => println!("→ One"),
                Match::Many(people) => println!("→ Many ({})", people.len()),
                Match::None => println!("→ None"),
            }
        }
        // What the loop's pre-gate lookup makes of `to`, then, with "dry_run": true, the tool run
        // on the result short of sending. Prints the outcome, never a whole handle.
        [cmd, name, json] if cmd == "prepare" => {
            let args: serde_json::Value = match serde_json::from_str(json) {
                Ok(args) => args,
                Err(e) => return println!("bad JSON arguments: {e}"),
            };
            let started = std::time::Instant::now();
            let resolved = match aloud_native::prepare(name, &args) {
                None => return println!("? {name} has no recipient to resolve"),
                Some(Err(e)) => {
                    let elapsed = started.elapsed();
                    return match e["candidates"].as_array() {
                        Some(names) => println!("→ Many ({}) in {elapsed:?}", names.len()),
                        None => println!("→ Err in {elapsed:?}: {}", e["error"]),
                    };
                }
                Some(Ok(resolved)) => resolved,
            };
            let to = resolved["to"].as_str().unwrap_or_default().to_string();
            match resolved["to_name"].as_str() {
                Some(who) => println!(
                    "→ One in {:?}: to_name {who:?}, to {}",
                    started.elapsed(),
                    mask(&to)
                ),
                None => println!("→ a handle, passed through: {}", mask(&to)),
            }
            if resolved["dry_run"] == true {
                match aloud_native::execute(name, &resolved) {
                    Some(Ok(mut out)) => {
                        out["to"] = json!(mask(&to));
                        println!("✓ dry run {out}");
                    }
                    Some(Err(e)) => {
                        println!("✗ dry run: {}", e.to_string().replace(&to, &mask(&to)))
                    }
                    None => {}
                }
            }
        }
        // Any tool with literal arguments: what the model would send, minus the model.
        [cmd, name, json] if cmd == "call" => match serde_json::from_str(json) {
            Ok(args) => call(name, args),
            Err(e) => println!("bad JSON arguments: {e}"),
        },
        other => println!("unknown: {other:?}"),
    }
}
