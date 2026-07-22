//! `aloud-hook` — the binary an agent spawns on every hook event.
//!
//! Reads one JSON payload on stdin, asks the app what to do, writes the
//! directive to stdout. That is the whole program.
//!
//! **It fails open, always.** No socket, no app, a malformed reply, a panic:
//! exit quietly with empty stdout and the agent proceeds exactly as though
//! nothing were installed. This runs inside other people's tools, on every tool
//! call they make, and the worst thing it could do is make their agent hang or
//! slow. Every error path here is `return`, never `unwrap`.
//!
//! ```sh
//! echo '{"hook_event_name":"PermissionRequest","tool_name":"Bash"}' \
//!   | aloud-hook --source claude
//! ```

use std::io::{Read, Write};

use aloud_bridge::{socket, Event, Source};

fn main() {
    // A panic must not print a backtrace to the agent's stdout, where it would
    // be parsed as a directive. Nothing here should panic; this is the belt.
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(run);
}

fn run() {
    // Before stdin is even read: another controller can own one run without
    // changing the global install state, and this is the first thing wanted
    // when two of these fight over the same agent.
    if std::env::var_os("ALOUD_SKIP_HOOKS").is_some_and(|v| !v.is_empty()) {
        return;
    }

    let Some(source) = source_flag() else { return };

    let mut stdin = String::new();
    if std::io::stdin().read_to_string(&mut stdin).is_err() {
        return;
    }
    let stdin = stdin.trim();
    if stdin.is_empty() {
        return;
    }

    let Ok(mut event) = serde_json::from_str::<Event>(stdin) else {
        return;
    };
    // The payload does not say which agent sent it; the flag does.
    event.source = Some(source);

    let Some(reply) = socket::request(&event, socket::timeout_for(&event)) else {
        return;
    };
    let Some(directive) = reply.directive(source) else {
        return;
    };
    let Ok(text) = serde_json::to_string(&directive) else {
        return;
    };

    // Any write failure is the agent having gone away, which is not ours to
    // report — there is nowhere left to report it to.
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(text.as_bytes());
    let _ = handle.flush();
}

/// Reads `--source <dialect>`.
///
/// Required, and unknown values are refused: without knowing the dialect the
/// directive would be encoded in the wrong shape, and a wrongly-shaped
/// directive is silently ignored by the agent. Refusing is the honest failure.
fn source_flag() -> Option<Source> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--source=") {
            return Source::parse(value);
        }
        if arg == "--source" {
            return Source::parse(&args.next()?);
        }
    }
    None
}
