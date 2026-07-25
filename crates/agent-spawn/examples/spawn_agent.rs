//! Real git + real Terminal, by hand — the OS-integration proof CLAUDE.md asks
//! for. Unit tests cover the pure parts; this is the only thing that proves the
//! branch checks out and Terminal actually opens.
//!
//! ```sh
//! cargo run -p aloud-agent-spawn --example spawn_agent -- <repo-path> <branch> "fix the thing"
//! ```
//!
//! Opens a Terminal tab that `cd`s into the repo, checks `<branch>` out there
//! (no worktree — the agent edits the repo directly), and runs `claude "<task>"`.

use std::path::Path;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let repo = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: spawn_agent <repo-path> <branch> <task…>"))?;
    let branch = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: spawn_agent <repo-path> <branch> <task…>"))?;
    let task: String = args.collect::<Vec<_>>().join(" ");
    anyhow::ensure!(!task.is_empty(), "give a task to run");

    println!(
        "branches: {:?}",
        aloud_agent_spawn::branches(Path::new(&repo))
    );

    let branch = aloud_agent_spawn::start_coding_agent(
        Path::new(&repo),
        &branch,
        &task,
        aloud_agent_spawn::Agent::Claude,
    )?;

    println!("branch:   {branch}");
    println!("Terminal should now be on {branch} in {repo}, running claude.");
    Ok(())
}
