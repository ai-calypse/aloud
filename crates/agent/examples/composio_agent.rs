//! Exercises the real Composio + Cerebras tool loop against live APIs, without
//! launching the app. Keys come from settings/.env exactly as the app loads
//! them (Groq/Cerebras/Composio).
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

const CEREBRAS_BASE_URL: &str = "https://api.cerebras.ai/v1";
const CEREBRAS_MODEL: &str = "gpt-oss-120b";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if command.trim().is_empty() {
        eprintln!("usage: composio_agent \"<spoken command>\"");
        std::process::exit(2);
    }

    let settings = Settings::load();
    if settings.cerebras_api_key.trim().is_empty() {
        anyhow::bail!("no CEREBRAS_API_KEY (settings or .env)");
    }
    if settings.composio_api_key.trim().is_empty() {
        anyhow::bail!("no COMPOSIO_API_KEY (settings or .env)");
    }

    let llm = aloud_llm::LlmClient::new(&settings.cerebras_api_key, Some(CEREBRAS_BASE_URL))?;
    let composio = ComposioClient::new(&settings.composio_api_key)?;

    println!("→ discovering connected Gmail/GitHub tools…");
    let tools = composio.available_tools().await?;
    if tools.is_empty() {
        anyhow::bail!("no Gmail/GitHub account connected in Composio");
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
        CEREBRAS_MODEL,
        &composio,
        &command,
        &tools,
        |event| match event {
            LoopEvent::ToolCallStart { name, args } => {
                println!("  [tool] {name}({})", args.join(", "));
            }
            LoopEvent::ToolCallResult { name, success } => {
                println!("  [{}] {name}", if success { "ok" } else { "ERR" });
            }
            LoopEvent::RateLimited { seconds } => {
                println!("  [rate-limited] retrying in {seconds}s");
            }
        },
        // No panel here: auto-approve writes. Reads dominate the test commands.
        |name, _args| async move {
            println!("  [auto-confirm write] {name}");
            true
        },
    )
    .await?;

    println!("\n=== answer ===\n{answer}");
    Ok(())
}
