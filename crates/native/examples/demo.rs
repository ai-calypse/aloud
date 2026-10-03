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
//! ```

use serde_json::json;

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
        // Any tool with literal arguments: what the model would send, minus the model.
        [cmd, name, json] if cmd == "call" => match serde_json::from_str(json) {
            Ok(args) => call(name, args),
            Err(e) => println!("bad JSON arguments: {e}"),
        },
        other => println!("unknown: {other:?}"),
    }
}
