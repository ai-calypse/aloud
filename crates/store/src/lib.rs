//! Settings, history and stats persistence.
//!
//! Replaces the Swift app's three-way split (UserDefaults + a `.settings`
//! JSON file + a Core Data store) with two files: `settings.json` and
//! `history.sqlite`, both in the app data directory.

pub mod history;
pub mod macros;
pub mod migrate;
pub mod rules;
pub mod settings;
pub mod stats;

pub use history::History;
pub use macros::Macro;
pub use rules::AppRule;
pub use settings::Settings;
pub use stats::Stats;
