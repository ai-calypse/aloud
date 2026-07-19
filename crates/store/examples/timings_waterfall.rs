//! Drives an utterance-shaped span tree through the *same* subscriber stack
//! the app builds in `lib.rs::run`, then prints the waterfall back.
//!
//! The unit test proves the layer. This proves the wiring: an `EnvFilter`
//! attached globally rather than per-layer would silently drop every span
//! before the timing layer saw it, and nothing else would notice.
//!
//! `cargo run -p aloud-store --example timings_waterfall`

use aloud_store::{TimingLayer, Timings};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::prelude::*;

/// Span names must be literals, so a macro rather than a function.
macro_rules! work {
    ($name:literal, $ms:expr) => {{
        let _s = tracing::info_span!($name).entered();
        std::thread::sleep(Duration::from_millis($ms));
    }};
}

fn main() -> anyhow::Result<()> {
    let db = Arc::new(Mutex::new(Timings::open_in_memory()?));

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "aloud=info,aloud_hotkey=info".into()),
            ),
        )
        .with(TimingLayer::new(db.clone()))
        .init();

    {
        let root = tracing::info_span!(
            "utterance",
            utterance_id = 1i64,
            spoken_ms = 2200u64,
            outcome = "pasted"
        );
        let _r = root.enter();

        work!("finalize_audio", 40);
        work!("transcribe", 380);
        {
            let cleanup = tracing::info_span!("cleanup", model = "llama-3.1-8b");
            let _c = cleanup.enter();
            std::thread::sleep(Duration::from_millis(60));
            // Nested: the retry that doubles perceived latency.
            work!("llm_fallback", 120);
            std::thread::sleep(Duration::from_millis(20));
        }
        work!("paste", 30);
        std::thread::sleep(Duration::from_millis(50)); // app overhead
    }

    // Agent mode: a second root, with the stages the dictation path never has.
    {
        let root = tracing::info_span!("utterance", utterance_id = 2i64, mode = "agent");
        let _r = root.enter();

        work!("finalize_audio", 30);
        work!("transcribe", 300);
        {
            let agent = tracing::info_span!("agent_run");
            let _a = agent.enter();
            work!("tool_discovery", 260);
            work!("tool_select", 410);
            // Human time. Must land on its own bar, not inside composio_exec.
            work!("confirm", 900);
            {
                let exec = tracing::info_span!("composio_exec", toolkit = "gmail", ok = true);
                let _e = exec.enter();
                std::thread::sleep(Duration::from_millis(640));
            }
            work!("tool_select", 220);
        }
    }

    let rows = db.lock().unwrap().recent(20)?;

    for id in [1, 2] {
        let mine: Vec<_> = rows.iter().filter(|r| r.utterance_id == id).collect();
        let root = mine.iter().find(|r| r.stage == "utterance").unwrap();
        println!(
            "\nutterance {} — {} ms total {}",
            root.utterance_id, root.duration_ms, root.fields
        );
        for row in mine.iter().filter(|r| r.stage != "utterance") {
            println!(
                "  {:>16}  +{:>5} ms  {:>5} ms  {}",
                row.stage,
                row.started_at - root.started_at,
                row.duration_ms,
                row.fields
            );
        }
    }

    assert_eq!(rows.len(), 6 + 9, "a span was dropped: {rows:?}");

    let dictation = rows
        .iter()
        .find(|r| r.stage == "utterance" && r.utterance_id == 1)
        .unwrap();
    assert!(dictation.duration_ms >= 700, "root must span its children");

    // Confirmation is its own bar, not swallowed by the tool it gates —
    // otherwise an approved write reads as a 1.5s Composio call.
    let confirm = rows.iter().find(|r| r.stage == "confirm").unwrap();
    let exec = rows.iter().find(|r| r.stage == "composio_exec").unwrap();
    assert!(confirm.duration_ms >= 900);
    assert!(
        exec.started_at >= confirm.started_at + confirm.duration_ms,
        "confirmation leaked into the exec span"
    );
    assert!(
        exec.fields.contains("gmail"),
        "toolkit not segmented: {}",
        exec.fields
    );

    // Two tool_select rounds: the reselect that doubles latency is visible.
    assert_eq!(rows.iter().filter(|r| r.stage == "tool_select").count(), 2);

    println!("\nok: both paths recorded, correlated, and confirmation kept separate");
    Ok(())
}
