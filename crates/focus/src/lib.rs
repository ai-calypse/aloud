//! The Focus section's logic, free of Tauri so it runs under `cargo test`
//! and from `examples/`: a timer that is a pure function of the wall clock,
//! `caffeinate` as a child process, and the islands both put in the notch.

pub mod awake;
pub mod island;
pub mod timer;

pub use awake::{Awake, AwakeState};
pub use timer::{
    Finished, Session, SessionKind, Timer, TimerMode, TimerPhase, TimerPreset, TimerState,
    TimerStatus,
};
