//! Live proof for the two things a stub cannot show: that a custom prompt
//! actually changes the model's output, and that a target language actually
//! translates.
//!
//!   GROQ_API_KEY=... cargo run -p aloud-llm --example prompt_and_language
//!
//! The substance of the Test Prompt runner and the output language setting —
//! both are `cleanup::clean` with one `Options` field overridden. Also walks
//! the two ways the runner looks broken while working: a sample under the skip
//! threshold, and a filler-only one.

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
        context: "",
        vocabulary: &[],
        output_language,
        instruction_guard: true,
    };

    let default = cleanup::clean(options("", ""), SAMPLE).await?;
    println!("default prompt    -> {default:?}");

    // An unmistakable signature, so "the prompt was ignored" cannot be mistaken
    // for "the model chose to write it that way".
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

    // Both surfaced as text rather than left to read as an empty panel.
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
