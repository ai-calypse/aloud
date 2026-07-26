//! Exercises screen control against a real, running app.
//!
//! Mandatory per `COMPUTER_USE.md`: this is exactly the category of code whose
//! unit tests pass while the feature does not work. The tests in `tree.rs` prove
//! the summariser filters correctly; only this proves the AX API returns real
//! elements and that clicking one does something.
//!
//! ```sh
//! cargo run -p aloud-computer --example screen              # read the focused app
//! cargo run -p aloud-computer --example screen -- click 420 300
//! cargo run -p aloud-computer --example screen -- type "hello"
//! cargo run -p aloud-computer --example screen -- key cmd+s
//! ```
//!
//! Focus the app you want to drive first — reading targets whatever is
//! frontmost, and a terminal in the foreground reads the terminal.

use serde_json::json;

fn main() {
    // To stderr, not stdout: this example prints the element JSON on stdout and
    // it is meant to be piped. tracing's default writer is stdout, which
    // interleaves a log line into the JSON and breaks every consumer of it.
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (tool, payload) = match args.as_slice() {
        [] => (aloud_computer::AX_READ, json!({})),
        [cmd, x, y] if cmd == "click" => (aloud_computer::AX_CLICK, json!({ "x": x, "y": y })),
        [cmd, text] if cmd == "type" => (aloud_computer::AX_TYPE, json!({ "text": text })),
        [cmd, keys] if cmd == "key" => (aloud_computer::AX_KEY, json!({ "keys": keys })),
        _ => {
            eprintln!("usage: screen [click <x> <y> | type <text> | key <chord>]");
            std::process::exit(2);
        }
    };

    if aloud_computer::tools().is_empty() {
        eprintln!(
            "No screen-control tools are available — the helpers did not build. \
             Check for a cargo:warning from aloud-computer's build script."
        );
        std::process::exit(1);
    }

    // Three seconds to focus the app being driven, since running this from a
    // terminal means the terminal is frontmost at the moment of the call.
    if tool != aloud_computer::AX_READ {
        eprintln!("acting in 3s — focus the target app now");
    }
    std::thread::sleep(std::time::Duration::from_secs(3));

    let started = std::time::Instant::now();
    match aloud_computer::execute(tool, &payload) {
        Some(Ok(value)) => {
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
            eprintln!("\n{tool} in {:?}", started.elapsed());
            if let Some(n) = value["elements"].as_array().map(Vec::len) {
                eprintln!(
                    "{n} actionable elements, {} bytes to the model",
                    value.to_string().len()
                );
            }
        }
        Some(Err(e)) => {
            eprintln!("{tool} failed: {e}");
            std::process::exit(1);
        }
        None => unreachable!("a name from this crate's own constants"),
    }
}
