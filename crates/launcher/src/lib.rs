//! Launching things from the notch: which apps are pinned or recent, running
//! an Apple Shortcut, and finding files. Every child process takes its
//! user-supplied text as one argv value, never through a shell or a script
//! body.
//!
//! No Tauri here; `cargo test -p aloud-launcher` covers the rules.

mod files;
mod prefs;
mod shortcuts;

pub use files::{recent_files, search_files, LaunchFile};
pub use prefs::{open_app, LauncherPrefs, PrefsStore};
pub use shortcuts::{list_shortcuts, run_shortcut};
