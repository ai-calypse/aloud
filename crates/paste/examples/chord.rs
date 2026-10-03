//! Presses one key chord on the real Mac and times it — the proof a unit test
//! cannot give that a synthesized shortcut reaches the system.
//!
//! `cargo run -p aloud-paste --example chord -- escape`
//! `cargo run -p aloud-paste --example chord -- cmd shift 5`   # the capture toolbar
//! `cargo run -p aloud-paste --example chord -- cmd shift code:23`   # by key code

use aloud_paste::{press_chord, ChordKey, Modifier};

fn main() -> anyhow::Result<()> {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let mut modifiers = Vec::new();
    let mut key = ChordKey::Code(53); // escape
    for word in &words {
        match word.as_str() {
            "cmd" => modifiers.push(Modifier::Command),
            "shift" => modifiers.push(Modifier::Shift),
            "option" => modifiers.push(Modifier::Option),
            "ctrl" => modifiers.push(Modifier::Control),
            "escape" => key = ChordKey::Code(53),
            code if code.starts_with("code:") => {
                key = ChordKey::Code(code[5..].parse().unwrap_or(53));
            }
            other => key = ChordKey::Char(other.chars().next().unwrap_or('a')),
        }
    }
    let started = std::time::Instant::now();
    press_chord(&modifiers, key)?;
    println!("pressed {words:?} in {:?}", started.elapsed());
    Ok(())
}
