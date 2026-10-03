//! Run one browser task end to end against the real Mac, approving any risky step from stdin.
//!
//! cargo run -p aloud-engine --example engine_task -- browser "What does serendipity mean? Check Wiktionary"
//! cargo run -p aloud-engine --example engine_task -- browser "search for cat memes" "cat memes"   # + query
//!
//! Uses the repo's `engine/` (run `npm install` there first, and build `engine/axd/axd`).

use aloud_engine::{Update, BROWSER_TASK};
use serde_json::json;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (kind, goal, query) = match args.as_slice() {
        [_, kind, goal] => (kind, goal, None),
        [_, kind, goal, query] => (kind, goal, Some(query)),
        _ => {
            eprintln!("usage: engine_task browser <goal> [query]");
            std::process::exit(2);
        }
    };
    if kind != "browser" {
        eprintln!("only browser tasks run on the engine now; Mac apps use aloud_native");
        std::process::exit(2);
    }
    let name = BROWSER_TASK;
    println!(
        "tools offered: {:?}",
        aloud_engine::tools()
            .iter()
            .map(|t| &t.name)
            .collect::<Vec<_>>()
    );
    let started = std::time::Instant::now();
    let mut task = match aloud_engine::start(name, &json!({ "goal": goal, "query": query })).await {
        Some(Ok(task)) => task,
        Some(Err(e)) => return eprintln!("couldn't start: {e}"),
        None => return eprintln!("not an engine tool"),
    };
    loop {
        match task.next().await {
            None => continue,
            Some(Update::Step(s)) => println!("  {s}"),
            Some(Update::NeedsApproval(q)) => {
                println!("  ? {q} [y/N]");
                let mut line = String::new();
                let _ = std::io::stdin().read_line(&mut line);
                let _ = task.approve(line.trim().eq_ignore_ascii_case("y")).await;
            }
            Some(Update::Finished(result)) => {
                println!(
                    "finished in {:.1}s:\n{}",
                    started.elapsed().as_secs_f32(),
                    serde_json::to_string_pretty(&result).unwrap_or_default()
                );
                break;
            }
        }
    }
}
