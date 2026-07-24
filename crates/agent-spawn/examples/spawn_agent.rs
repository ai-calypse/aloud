//! Real git + real Terminal, by hand — the OS-integration proof CLAUDE.md asks
//! for. Unit tests cover the pure parts; this is the only thing that proves the
//! worktree actually appears and Terminal actually opens.
//!
//! ```sh
//! cargo run -p aloud-agent-spawn --example spawn_agent -- <repo-path> <branch> "fix the thing"
//! ```
//!
//! Checks `<branch>` out into `<repo>-<branch>` beside the repo and opens a
//! Terminal tab running `claude "<task>"` there. Clean up after with
//! `git worktree remove <path>` (the branch itself is left alone).

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

    println!("branches: {:?}", aloud_agent_spawn::branches(Path::new(&repo)));

    let branch = aloud_agent_spawn::start_coding_agent(
        Path::new(&repo),
        &branch,
        &task,
        aloud_agent_spawn::Agent::Claude,
    )?;

    let path = aloud_agent_spawn::worktree_path(Path::new(&repo), &branch.replace('/', "-"));
    println!("worktree: {}", path.display());
    println!("branch:   {branch}");
    println!("Terminal should now be running claude in that worktree.");
    println!("clean up: git worktree remove {}", path.display());
    Ok(())
}
