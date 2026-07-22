//! Settings, history and stats persistence: `settings.json` and
//! `history.sqlite` in the app data directory.

pub mod history;
pub mod macros;
pub mod migrate;
pub mod rules;
pub mod settings;
pub mod stats;
pub mod timings;

pub use history::History;
pub use macros::Macro;
pub use rules::AppRule;
pub use settings::Settings;
pub use stats::Stats;
pub use timings::{TimingLayer, Timings};
