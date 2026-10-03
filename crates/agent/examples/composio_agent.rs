//! Exercises the real Composio + Gemini tool loop against live APIs, without
//! launching the app. Keys come from settings/.env exactly as the app loads
//! them (Groq/Gemini/Composio).
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

const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
const GEMINI_MODEL: &str = "gemini-3.1-flash-lite";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if command.trim().is_empty() {
        eprintln!("usage: composio_agent \"<spoken command>\"");
        std::process::exit(2);
    }

    let settings = Settings::load();
    if settings.gemini_api_key.trim().is_empty() {
        anyhow::bail!("no GEMINI_API_KEY (settings or .env)");
    }
    if settings.composio_api_key.trim().is_empty() {
        anyhow::bail!("no COMPOSIO_API_KEY (settings or .env)");
    }

    let llm = aloud_llm::LlmClient::new(&settings.gemini_api_key, Some(GEMINI_BASE_URL))?;
    let composio = ComposioClient::new(&settings.composio_api_key)?;

    println!("→ selecting connected tools for this command…");
    let tools = composio.available_tools(&command).await?;
    if tools.is_empty() {
        anyhow::bail!("no account connected in Composio");
    }
    println!(
        "  tools available: {}",
        tools
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    println!("→ command: {command}\n");
    let answer = run_tool_loop(
        &llm,
        GEMINI_MODEL,
        &composio,
        &command,
        &tools,
        |event| match event {
            LoopEvent::Planned { names } => {
                println!("  [plan] {}", names.join(" → "));
            }
            LoopEvent::ToolCallStart { name, args } => {
                println!("  [tool] {name}({args})");
            }
            LoopEvent::ToolCallResult { name, success } => {
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
        |name, args: serde_json::Value| async move {
            println!("  [auto-confirm write] {name}");
            Some(args)
        },
        // No panel to type into: every question goes unanswered, which the loop
        // reports to the model so it closes the run rather than hanging.
        |question: String| async move {
            println!("  [asked, unanswered] {question}");
            None
        },
        // Nothing to cancel from here: the example runs to completion or errors.
        || false,
    )
    .await?;

    println!("\n=== answer ===\n{answer}");
    Ok(())
}
