//! Clipboard history, snippets and picked colours for the notch's Clipboard
//! section. Local by construction: entries live in one SQLite file and a
//! folder of PNGs under the app's data directory, are never logged, and are
//! never given to a model. History is off until the user turns it on.
//!
//! No Tauri here — the app wires `Store` to commands and `watch::spawn` to an
//! event — so every rule is testable with `cargo test -p aloud-clipboard`.

pub mod actions;
pub mod colors;
mod entry;
mod ocr;
mod store;
pub mod watch;

pub use entry::{
    content_hash, is_concealed, reformat_json, text_actions, Captured, ClipboardEntry,
    ClipboardKind, SmartAction,
};
pub use store::{Content, PickedColor, Snippet, Store, MAX_ENTRIES};
