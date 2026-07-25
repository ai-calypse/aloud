//! Lists the native tools available on this machine, then runs one against the
//! real app — the only proof that the AppleScript actually works, since unit
//! tests cannot reach Music.app.
//!
//! ```sh
//! cargo run -p aloud-native --example native                 # list + now-playing
//! cargo run -p aloud-native --example native -- play         # resume playback
//! cargo run -p aloud-native --example native -- play "bowie" # search + play
//! ```

use serde_json::json;

fn main() {
    tracing_subscriber::fmt().init();

    let installed = aloud_selection::installed_apps();
    let tools = aloud_native::tools(&installed);

    println!("\n{} native tool(s) on this machine:", tools.len());
    for tool in &tools {
        let kind = if tool.read_only { "read " } else { "write" };
        println!("  [{kind}] {} — {}", tool.name, tool.description);
    }
    if tools.is_empty() {
        println!("  (none — is Music installed?)");
        return;
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (name, params) = match args.first().map(String::as_str) {
        Some("play") => {
            let query = args.get(1).cloned().unwrap_or_default();
            ("ALOUD_MUSIC_PLAY", json!({ "query": query }))
        }
        Some("search") => {
            let query = args.get(1).cloned().unwrap_or_else(|| "aloud".into());
            let kind = args.get(2).cloned().unwrap_or_else(|| "image".into());
            ("ALOUD_FINDER_SEARCH", json!({ "query": query, "kind": kind }))
        }
        _ => ("ALOUD_MUSIC_NOW_PLAYING", json!({})),
    };

    println!("\nrunning {name} with {params}...");
    match aloud_native::execute(name, &params) {
        Some(Ok(value)) => println!("→ {value}"),
        Some(Err(e)) => println!("✗ {e}"),
        None => println!("✗ {name} is not a native tool"),
    }
}
