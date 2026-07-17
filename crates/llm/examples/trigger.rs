//! The routing decision the pipeline makes on every transcript: ask or paste?
//!
//! `cargo run -p aloud-llm --example trigger`

fn main() {
    for transcript in [
        "Hey Flow, what does this error mean?",
        "Ask AI how do I exit vim",
        "Send hey flow to the team",
        "just some normal dictation",
    ] {
        match aloud_llm::ask::question_from_trigger(transcript) {
            Some(question) => println!("ASK   {transcript:?}\n   -> {question:?}"),
            None => println!("PASTE {transcript:?}"),
        }
    }
}
