//! Read-only probe of the connector paths against the live Composio API.
use aloud_store::Settings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = Settings::load();
    let c = aloud_agent::ComposioClient::new(&settings.composio_api_key)?;

    println!("== connected_toolkits ==");
    for t in c.connected_toolkits().await? {
        println!("  {} ({}) — {} tools total", t.name, t.slug, t.tools_count);
        for tool in c
            .toolkit_tools(&t.slug, aloud_agent::TOOLS_PER_TOOLKIT)
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
    let tools = c.available_tools().await?;
    println!("  {} tools", tools.len());
    Ok(())
}
