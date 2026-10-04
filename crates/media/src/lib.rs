//! The notch's Media section, minus the pixels: what Spotify or Music is
//! playing, its lyrics, the audio devices, and the batteries of the Bluetooth
//! gear on the desk. No Tauri here; `src-tauri/src/media.rs` wires it.
//!
//! macOS-only in substance. Every other platform gets the same API answering
//! "nothing", the way `aloud_volume` does.

pub mod artwork;
pub mod devices;
pub mod lyrics;
pub mod now_playing;
pub mod sound;

pub use artwork::artwork;
pub use now_playing::{NowPlaying, PlayState, Player, Reading, Transport};
