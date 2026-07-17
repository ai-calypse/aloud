//! Exercises what the Settings UI does: load, change a field, save, reload.
//!
//! `cargo run -p aloud-store --example roundtrip`

use aloud_store::Settings;

fn main() -> anyhow::Result<()> {
    let before = Settings::load();
    println!(
        "loaded:   engine={:?} key_set={}",
        before.engine,
        !before.groq_api_key.is_empty()
    );

    let next = Settings {
        vocabulary: vec!["Kubernetes".into(), "Tauri".into()],
        history_retention: 25,
        ..before.clone()
    };
    next.save()?;

    let after = Settings::load();
    println!(
        "reloaded: vocabulary={:?} retention={}",
        after.vocabulary, after.history_retention
    );
    assert_eq!(after.vocabulary, next.vocabulary);
    assert_eq!(after.history_retention, 25);
    assert_eq!(after.groq_api_key, before.groq_api_key, "keys must survive");

    // Restore, so running this never leaves real settings altered.
    before.save()?;
    println!("restored: vocabulary={:?}", Settings::load().vocabulary);
    Ok(())
}
