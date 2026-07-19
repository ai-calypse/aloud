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

    let rows = db.lock().unwrap().recent(20)?;
    let root = rows.iter().find(|r| r.stage == "utterance").unwrap();
    println!(
        "\nutterance {} — {} ms total",
        root.utterance_id, root.duration_ms
    );
    for row in rows.iter().filter(|r| r.stage != "utterance") {
        println!(
            "  {:>14}  +{:>4} ms  {:>4} ms  {}",
            row.stage,
            row.started_at - root.started_at,
            row.duration_ms,
            row.fields
        );
    }

    assert_eq!(rows.len(), 6, "a span was dropped: {rows:?}");
    assert!(rows.iter().all(|r| r.utterance_id == 1));
    assert!(root.duration_ms >= 700, "root must span its children");
    println!("\nok: every stage recorded and correlated");
    Ok(())
}
