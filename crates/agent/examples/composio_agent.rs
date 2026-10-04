//! Exercises the real Composio tool loop against live APIs, without launching
//! the app, on the same brains the app uses (Cerebras, then Gemini). Keys come
//! from settings/.env exactly as the app loads them.
//!
//! Run from the repo root so the `.env` walk finds the root file:
//!
//!   cargo run -p aloud-agent --example composio_agent -- "what is my github username"
//!   cargo run -p aloud-agent --example composio_agent -- "summarize my last 4 emails"
//!
//! Writes (send email, create issue) auto-confirm here — there is no panel — so
//! don't run a "send an email" command unless you mean it.

use aloud_agent::{run_tool_loop, ComposioClient, LoopEvent};
use aloud_store::Settings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if command.trim().is_empty() {
        eprintln!("usage: composio_agent \"<spoken command>\"");
        std::process::exit(2);
    }

    let settings = Settings::load();
    let brains = aloud_agent::brains(&settings.groq_api_key, &settings.gemini_api_key)?;
    if brains.is_empty() {
        anyhow::bail!("no GROQ_API_KEY or GEMINI_API_KEY (settings or .env)");
    }
    if settings.composio_api_key.trim().is_empty() {
        anyhow::bail!("no COMPOSIO_API_KEY (settings or .env)");
    }

    let composio = ComposioClient::new(&settings.composio_api_key)?;

    // The same tools the app offers: the Mac's own, the browser task, and the
    // connected services the command is about.
    println!("→ selecting tools for this command…");
    let installed = aloud_selection::installed_apps();
    let aloud_agent::CommandTools { tools, more } =
        aloud_agent::command_tools(&command, &installed, Some(&composio)).await;
    println!(
        "  tools available: {}",
        tools
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    println!("→ command: {command}\n");
    let started = std::time::Instant::now();
    let answer = run_tool_loop(
        &brains,
        &composio,
        aloud_agent::Command {
            said: &command,
            prompt: &command,
            tools: &tools,
            more: &more,
            front: None,
            document: None,
            project: None,
            selection: false,
        },
        aloud_agent::Callbacks {
            on_event: |event| match event {
                LoopEvent::Planned { names } => {
                    println!("  [plan] {}", names.join(" → "));
                }
                LoopEvent::ToolCallStart { name, args } => {
                    println!("  [tool] {name}({args})");
                }
                LoopEvent::ToolCallResult { name, success, .. } => {
                    println!("  [{}] {name}", if success { "ok" } else { "ERR" });
                }
                LoopEvent::Asked { question, options } => {
                    println!("  [ask] {question} {options:?}");
                }
                LoopEvent::RateLimited { seconds } => {
                    println!("  [rate-limited] retrying in {seconds}s");
                }
                LoopEvent::Progress { text, .. } => println!("    · {text}"),
            },
            // No panel here: auto-approve writes with the arguments as-is. Reads
            // dominate the test commands.
            confirm: |call: aloud_agent::ProposedCall| async move {
                println!("  [auto-confirm write] {}", call.name);
                aloud_agent::Verdict::Run(call.args)
            },
            // No panel to type into: every question goes unanswered, which the
            // loop reports to the model so it closes the run rather than hanging.
            ask: |question: String| async move {
                println!("  [asked, unanswered] {question}");
                None
            },
            // Nothing to cancel from here: the example runs to completion or errors.
            cancelled: || false,
            mock: None,
        },
    )
    .await?;

    println!(
        "\n=== answer ({:.2}s) ===\n{answer}",
        started.elapsed().as_secs_f32()
    );
    Ok(())
}
