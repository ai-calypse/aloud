//! Read-only probe of the connector paths against the live Composio API. Pass a
//! command to see which tools it selects — per-command ranking means this output
//! changes with the words:
//!
//!   cargo run -p aloud-agent --example connectors_probe
//!   cargo run -p aloud-agent --example connectors_probe -- "delete the netflix event from my calendar"
use aloud_store::Settings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let settings = Settings::load();
    let c = aloud_agent::ComposioClient::new(&settings.composio_api_key)?;

    println!("== connected_toolkits ==");
    println!("   command: {:?}\n", command);
    for t in c.connected_toolkits().await? {
        println!("  {} ({}) — {} tools total", t.name, t.slug, t.tools_count);
        for tool in c
            .toolkit_tools(&t.slug, &command, aloud_agent::TOOLS_PER_TOOLKIT)
            .await?
        {
            println!(
                "     {} {}",
                if tool.read_only { "READ " } else { "WRITE" },
                tool.name
            );
        }
    }

    println!("\n== search 'slack' ==");
    for t in c.toolkits("slack").await?.iter().take(4) {
        println!(
            "  {} ({}) connected={} tools={}",
            t.name, t.slug, t.connected, t.tools_count
        );
    }

    println!("\n== available_tools (what the agent actually gets) ==");
    let tools = c.available_tools(&command).await?;
    println!("  {} tools", tools.len());
    Ok(())
}
