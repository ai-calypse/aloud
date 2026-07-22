//! Measures what a dictation actually waits for on the cleanup stage.
//!
//! Two things unit tests cannot show: connection reuse (a per-request
//! `LlmClient` paid a fresh TCP+TLS handshake every dictation before the pool
//! was process-wide — run 1 pays it, runs 2+ must not), and reasoning tokens
//! (the default cleanup model reasons unless told not to).
//!
//!   GROQ_API_KEY=... cargo run -p aloud-llm --example cleanup_latency
//!
//! Needs a live key and network. Reports each run so a single slow sample is
//! visible rather than averaged away.

use aloud_llm::cleanup;
use std::time::Instant;

const SAMPLE: &str = "um so i was thinking we should probably uh ship the \
    thing on friday but only if the tests pass you know";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Ok(api_key) = std::env::var("GROQ_API_KEY") else {
        eprintln!("set GROQ_API_KEY (the repo's .env has one)");
        std::process::exit(1);
    };

    let model = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "openai/gpt-oss-20b".into());
    println!("model: {model}\n");

    let mut timings = Vec::new();
    for run in 1..=4 {
        let options = cleanup::Options {
            api_key: &api_key,
            base_url: aloud_llm::DEFAULT_BASE_URL,
            model: &model,
            fallback: None,
            custom_prompt: "",
            context: "",
            vocabulary: &[],
            output_language: "",
            instruction_guard: true,
        };

        let started = Instant::now();
        let result = cleanup::clean(options, SAMPLE).await;
        let elapsed = started.elapsed();
        timings.push(elapsed);

        match result {
            Ok(text) => println!("run {run}: {:>7.0}ms  {text:?}", elapsed.as_millis()),
            Err(e) => println!("run {run}: {:>7.0}ms  FAILED: {e}", elapsed.as_millis()),
        }
    }

    // A spread, not one number: server variance means any single run proves
    // nothing. Compare the mean between builds — run 1 alone cannot show the
    // handshake, because a slow completion swamps the ~80ms it costs.
    let mut ms: Vec<u128> = timings.iter().map(|d| d.as_millis()).collect();
    let mean = ms.iter().sum::<u128>() / ms.len() as u128;
    ms.sort_unstable();
    println!(
        "\nmean {mean}ms · min {}ms · max {}ms over {} runs",
        ms[0],
        ms[ms.len() - 1],
        ms.len()
    );
    Ok(())
}
