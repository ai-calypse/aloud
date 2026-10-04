//! Read-only probe of the connector paths against the live Composio API. Pass a
//! command to see which tools it selects — per-command ranking means this output
//! changes with the words:
//!
//!   cargo run -p aloud-agent --example connectors_probe
//!   cargo run -p aloud-agent --example connectors_probe -- "delete the netflix event from my calendar"
//!
//! `previews` instead checks `preview/hand.json` against the live schemas: every
//! connector it maps, reads or offers must exist, and every pointer must land
//! on a real field. Refresh `preview/fixtures/connectors.json` when it fails.
//!
//!   cargo run -p aloud-agent --example connectors_probe -- previews
use aloud_store::Settings;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let settings = Settings::load();
    let c = aloud_agent::ComposioClient::new(&settings.composio_api_key)?;
    if command == "previews" {
        return previews(&c).await;
    }

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

async fn previews(c: &aloud_agent::ComposioClient) -> anyhow::Result<()> {
    use aloud_agent::preview::{check_hand, hand_tools};
    let slugs: Vec<&str> = hand_tools()
        .into_iter()
        .filter(|slug| !slug.starts_with("ALOUD_"))
        .collect();
    let mut live = HashMap::new();
    // Small pages: the slug filter is still paginated.
    for chunk in slugs.chunks(10) {
        for tool in c.tools_by_slug(&chunk.join(",")).await? {
            live.insert(tool.name, tool.parameters);
        }
    }
    println!("== hand.json against live schemas ==");
    for slug in &slugs {
        let state = if live.contains_key(*slug) {
            "live   "
        } else {
            "MISSING"
        };
        println!("  {state} {slug}");
    }
    // Native tools are checked against their own schemas by the unit test
    // `hand_mappings_parse_and_point_at_real_fields`.
    let problems: Vec<String> = check_hand(|name| live.get(name).cloned())
        .into_iter()
        .filter(|p| !p.starts_with("ALOUD_"))
        .collect();
    for problem in &problems {
        println!("  ✗ {problem}");
    }
    anyhow::ensure!(problems.is_empty(), "{} problems", problems.len());
    println!("  every connector pointer lands on a live field");
    Ok(())
}
