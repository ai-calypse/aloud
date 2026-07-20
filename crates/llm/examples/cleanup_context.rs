//! Live proof that the screen context actually reaches the model and changes
//! what gets pasted.
//!
//!   GROQ_API_KEY=... cargo run -p aloud-llm --example cleanup_context
//!
//! `prompts/cleanup.md` has carried the rule since it was ported — "correct a
//! near-match like 'Aisha' to the visible recipient spelling 'Aysha' when it
//! is clearly the same intended person" — but nothing ever supplied a context,
//! so it was dead text. A unit test can only assert the string was built; only
//! a real request shows the model obeying the rule.
//!
//! The sample deliberately mis-spells a name the way Whisper would. Without
//! context the model has nothing to correct against and must leave it alone;
//! with the mail window's title it must fix the spelling — and must NOT invent
//! the surname, which the prompt forbids.

use aloud_llm::cleanup;

/// Phonetically what was said. The recipient's real spelling appears only in
/// the window title, never in the transcript.
const SAMPLE: &str = "hi aisha just confirming that thursday afternoon still \
    works on my end for the q3 planning review";

const CONTEXT: &str = "Mail — Re: Q3 planning review — Aysha Rahman";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Ok(api_key) = std::env::var("GROQ_API_KEY") else {
        eprintln!("set GROQ_API_KEY (the repo's .env has one)");
        std::process::exit(1);
    };

    let options = |context| cleanup::Options {
        api_key: &api_key,
        base_url: aloud_llm::DEFAULT_BASE_URL,
        model: "openai/gpt-oss-20b",
        fallback: None,
        custom_prompt: "",
        vocabulary: &[],
        context,
        output_language: "",
        instruction_guard: true,
    };

    let without = cleanup::clean(options(""), SAMPLE).await?;
    println!("no context   -> {without:?}");

    let with = cleanup::clean(options(CONTEXT), SAMPLE).await?;
    println!("with context -> {with:?}");

    assert!(
        with.contains("Aysha"),
        "the context named the recipient and the spelling was not adopted"
    );
    // The prompt's other half: a visible name is a spelling reference, not
    // content to add. Pasting a surname the user never said is worse than
    // pasting the wrong spelling, because they cannot see it was invented.
    assert!(
        !with.contains("Rahman"),
        "a surname that was never spoken was pulled out of the context"
    );
    println!("\nok: the spelling was corrected and the surname was not invented");
    Ok(())
}
