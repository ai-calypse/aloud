//! Live proof for the two things a stub cannot show: that a custom prompt
//! actually changes the model's output, and that a target language actually
//! translates.
//!
//!   GROQ_API_KEY=... cargo run -p aloud-llm --example prompt_and_language
//!
//! This is the substance of the Test Prompt runner (3.1) and of the output
//! language setting (3.4) — both are `cleanup::clean` with one field of
//! `Options` overridden, so exercising them here exercises them everywhere.
//! It also walks the two ways the runner can look broken while working
//! correctly: a sample under the skip threshold, and a filler-only sample.

use aloud_llm::cleanup;

const SAMPLE: &str = "um so i was thinking we should probably uh ship the \
    thing on friday but only if the tests pass you know";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Ok(api_key) = std::env::var("GROQ_API_KEY") else {
        eprintln!("set GROQ_API_KEY (the repo's .env has one)");
        std::process::exit(1);
    };

    let options = |custom_prompt, output_language| cleanup::Options {
        api_key: &api_key,
        base_url: aloud_llm::DEFAULT_BASE_URL,
        model: "openai/gpt-oss-20b",
        fallback: None,
        custom_prompt,
        vocabulary: &[],
        output_language,
    };

    let default = cleanup::clean(options("", ""), SAMPLE).await?;
    println!("default prompt    -> {default:?}");

    // A prompt with an unmistakable signature, so "the custom prompt was
    // ignored" cannot be mistaken for "the model chose to write it that way".
    let custom = cleanup::clean(
        options(
            "Rewrite the user's transcript as a single ALL-CAPS sentence of at \
             most eight words. Output only that sentence.",
            "",
        ),
        SAMPLE,
    )
    .await?;
    println!("custom prompt     -> {custom:?}");
    assert_ne!(custom, default, "the custom prompt changed nothing");

    let german = cleanup::clean(options("", "German"), SAMPLE).await?;
    println!("output: German    -> {german:?}");
    assert_ne!(german, default, "the target language changed nothing");

    // The two ways the runner looks broken while behaving. Both are surfaced
    // as text by the command rather than left to read as an empty panel.
    println!(
        "\nunder the threshold: {:?} -> skipped, {} words <= MIN_WORDS {}",
        "send it now",
        "send it now".split_whitespace().count(),
        cleanup::MIN_WORDS
    );
    assert!(cleanup::is_too_short("send it now"));

    let filler = cleanup::clean(options("", ""), "um uh er hmm you know like").await?;
    println!("filler only       -> {filler:?} (EMPTY sentinel means paste nothing)");

    println!("\nok: custom prompts and target languages both reach the model");
    Ok(())
}
