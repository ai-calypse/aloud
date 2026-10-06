//! A Pomodoro / stopwatch timer as a state machine over wall-clock
//! milliseconds.
//!
//! Nothing here ticks. Every method takes `now` (unix ms) and the answer is
//! derived from it, so a laptop that slept for forty minutes wakes up with the
//! right time on the clock: a monotonic `Instant` does not advance during
//! sleep on macOS, and a counter decremented once a second would be forty
//! minutes behind. The app's one-second task only *asks*; it never counts.

use serde::{Deserialize, Serialize};

/// Focus rounds before the long break.
pub const ROUNDS: u32 = 4;

/// A stopwatch run shorter than this is a fumble, not a session.
const MIN_STOPWATCH_SECS: i64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TimerMode {
    Pomodoro,
    Stopwatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TimerPhase {
    Focus,
    ShortBreak,
    LongBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TimerStatus {
    Idle,
    Running,
    Paused,
    /// A phase just ended and the next one waits for the user: coming back
    /// from lunch must not find the whole break already spent.
    Ready,
}

/// Minutes, so the two common rhythms are one tap each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TimerPreset {
    pub focus: u32,
    pub rest: u32,
}

impl Default for TimerPreset {
    fn default() -> Self {
        Self { focus: 25, rest: 5 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SessionKind {
    Pomodoro,
    Stopwatch,
}

/// Time worth counting toward a streak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub kind: SessionKind,
    pub secs: u32,
}

/// What a tick found: a phase ran out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finished {
    pub phase: TimerPhase,
    /// Some for a focus phase that ran to the end.
    pub session: Option<Session>,
    /// What waits now.
    pub next: TimerPhase,
}

/// Everything the webview needs to draw the timer, with no clock of its own to
/// drift: it reads `Date.now()` against `at` / `held_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TimerState {
    pub mode: TimerMode,
    pub phase: TimerPhase,
    pub status: TimerStatus,
    /// 1…=`rounds`.
    pub round: u32,
    pub rounds: u32,
    pub preset: TimerPreset,
    /// Running: when a countdown ends, or when a stopwatch (net of pauses)
    /// began, in unix ms.
    #[specta(type = specta_typescript::Number)]
    pub at: i64,
    /// Not running: milliseconds left (countdown) or elapsed (stopwatch).
    #[specta(type = specta_typescript::Number)]
    pub held_ms: i64,
    /// The phase's full length, for the ring; 0 for a stopwatch.
    #[specta(type = specta_typescript::Number)]
    pub total_ms: i64,
}

impl TimerState {
    /// Milliseconds on the face: left for a countdown, elapsed for a
    /// stopwatch. Mirrored by `lib/focus.ts`.
    pub fn value_ms(&self, now: i64) -> i64 {
        match (self.status, self.mode) {
            (TimerStatus::Running, TimerMode::Pomodoro) => (self.at - now).clamp(0, self.total_ms),
            (TimerStatus::Running, TimerMode::Stopwatch) => (now - self.at).max(0),
            _ => self.held_ms,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Timer {
    state: TimerState,
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

fn phase_ms(phase: TimerPhase, preset: TimerPreset) -> i64 {
    let minutes = match phase {
        TimerPhase::Focus => preset.focus,
        TimerPhase::ShortBreak => preset.rest,
        TimerPhase::LongBreak => preset.rest * 3,
    };
    i64::from(minutes.max(1)) * 60_000
}

impl Timer {
    pub fn new() -> Self {
        Self {
            state: TimerState {
                mode: TimerMode::Pomodoro,
                phase: TimerPhase::Focus,
                status: TimerStatus::Idle,
                round: 1,
                rounds: ROUNDS,
                preset: TimerPreset::default(),
                at: 0,
                held_ms: 0,
                total_ms: 0,
            },
        }
    }

    pub fn state(&self) -> TimerState {
        self.state
    }

    pub fn is_running(&self) -> bool {
        self.state.status == TimerStatus::Running
    }

    /// Begins a fresh run, replacing whatever was there (the caller decides
    /// whether a half-done run needed confirming).
    pub fn start(&mut self, mode: TimerMode, preset: TimerPreset, now: i64) {
        let preset = TimerPreset {
            focus: preset.focus.clamp(1, 180),
            rest: preset.rest.clamp(1, 60),
        };
        let total = if mode == TimerMode::Pomodoro {
            phase_ms(TimerPhase::Focus, preset)
        } else {
            0
        };
        self.state = TimerState {
            mode,
            phase: TimerPhase::Focus,
            status: TimerStatus::Running,
            round: 1,
            rounds: ROUNDS,
            preset,
            at: if mode == TimerMode::Pomodoro {
                now + total
            } else {
                now
            },
            held_ms: 0,
            total_ms: total,
        };
    }

    pub fn pause(&mut self, now: i64) {
        if self.state.status != TimerStatus::Running {
            return;
        }
        self.state.held_ms = self.state.value_ms(now);
        self.state.status = TimerStatus::Paused;
    }

    /// Resumes a pause, or starts the phase that is waiting.
    pub fn resume(&mut self, now: i64) {
        let s = &mut self.state;
        if !matches!(s.status, TimerStatus::Paused | TimerStatus::Ready) {
            return;
        }
        s.at = match s.mode {
            TimerMode::Pomodoro => now + s.held_ms,
            TimerMode::Stopwatch => now - s.held_ms,
        };
        s.status = TimerStatus::Running;
    }

    /// Ends the phase now without counting it, and queues the next one.
    pub fn skip(&mut self) {
        if self.state.mode == TimerMode::Pomodoro && self.state.status != TimerStatus::Idle {
            self.advance();
        }
    }

    /// Stops and clears. A stopwatch that ran a real while hands back its
    /// session; a half-spent pomodoro does not count.
    pub fn stop(&mut self, now: i64) -> Option<Session> {
        let session = (self.state.mode == TimerMode::Stopwatch
            && self.state.status != TimerStatus::Idle)
            .then(|| self.state.value_ms(now) / 1000)
            .filter(|secs| *secs >= MIN_STOPWATCH_SECS)
            .map(|secs| Session {
                kind: SessionKind::Stopwatch,
                secs: secs as u32,
            });
        *self = Self::new();
        session
    }

    /// Asked once a second (and on wake). A phase that ran out — even long
    /// ago, while the machine slept — ends exactly once and *waits*; it never
    /// chains on through the breaks it slept past.
    pub fn tick(&mut self, now: i64) -> Option<Finished> {
        let s = self.state;
        if s.mode != TimerMode::Pomodoro || s.status != TimerStatus::Running || now < s.at {
            return None;
        }
        let session = (s.phase == TimerPhase::Focus).then_some(Session {
            kind: SessionKind::Pomodoro,
            secs: (s.total_ms / 1000) as u32,
        });
        self.advance();
        Some(Finished {
            phase: s.phase,
            session,
            next: self.state.phase,
        })
    }

    /// Moves to the phase after this one, held at its full length.
    fn advance(&mut self) {
        let s = &mut self.state;
        let (phase, round) = match s.phase {
            TimerPhase::Focus if s.round >= s.rounds => (TimerPhase::LongBreak, s.round),
            TimerPhase::Focus => (TimerPhase::ShortBreak, s.round),
            TimerPhase::ShortBreak => (TimerPhase::Focus, s.round + 1),
            TimerPhase::LongBreak => (TimerPhase::Focus, 1),
        };
        s.phase = phase;
        s.round = round;
        s.status = TimerStatus::Ready;
        s.total_ms = phase_ms(phase, s.preset);
        s.held_ms = s.total_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

    fn running(now: i64) -> Timer {
        let mut t = Timer::new();
        t.start(TimerMode::Pomodoro, TimerPreset::default(), now);
        t
    }

    /// The failure a one-second counter has: the lid closes at minute 10, opens
    /// at minute 20, and the face still says 15 left.
    #[test]
    fn the_clock_keeps_time_across_sleep() {
        let t = running(0);
        assert_eq!(t.state().value_ms(10 * MIN), 15 * MIN);
        // Asleep from minute 10 to 20, no tick in between.
        assert_eq!(t.state().value_ms(20 * MIN), 5 * MIN);
    }

    /// Sleeping through the end — and through the break after it — must end
    /// the focus once and stop at the break, not burn it.
    #[test]
    fn sleeping_past_the_end_finishes_once_and_waits() {
        let mut t = running(0);
        let done = t.tick(8 * 60 * MIN).expect("focus ended");
        assert_eq!(done.phase, TimerPhase::Focus);
        assert_eq!(
            done.session,
            Some(Session {
                kind: SessionKind::Pomodoro,
                secs: 25 * 60
            })
        );
        assert_eq!(done.next, TimerPhase::ShortBreak);
        let s = t.state();
        assert_eq!(
            (s.status, s.phase, s.value_ms(9 * 60 * MIN)),
            (TimerStatus::Ready, TimerPhase::ShortBreak, 5 * MIN)
        );
        assert_eq!(
            t.tick(9 * 60 * MIN),
            None,
            "a waiting phase does not end again"
        );
    }

    #[test]
    fn a_pause_holds_the_time_and_resume_picks_it_up() {
        let mut t = running(0);
        t.pause(10 * MIN);
        // An hour of lunch.
        assert_eq!(t.state().value_ms(70 * MIN), 15 * MIN);
        assert_eq!(t.tick(70 * MIN), None);
        t.resume(70 * MIN);
        assert_eq!(t.state().value_ms(75 * MIN), 10 * MIN);
        assert!(t.tick(85 * MIN - 1).is_none());
        assert!(t.tick(85 * MIN).is_some());
    }

    #[test]
    fn four_rounds_then_a_long_break_then_round_one() {
        let mut t = running(0);
        let mut seen = Vec::new();
        for _ in 0..8 {
            let now = t.state().at + 1;
            let round = t.state().round;
            let f = t.tick(now).unwrap();
            seen.push((f.phase, round));
            t.resume(now);
        }
        use TimerPhase::*;
        assert_eq!(
            seen,
            [
                (Focus, 1),
                (ShortBreak, 1),
                (Focus, 2),
                (ShortBreak, 2),
                (Focus, 3),
                (ShortBreak, 3),
                (Focus, 4),
                (LongBreak, 4),
            ]
        );
        assert_eq!((t.state().phase, t.state().round), (TimerPhase::Focus, 1));
        assert_eq!(t.state().total_ms, 25 * MIN);
    }

    #[test]
    fn skipping_a_focus_does_not_count_it() {
        let mut t = running(0);
        t.skip();
        assert_eq!(t.state().phase, TimerPhase::ShortBreak);
        assert_eq!(t.state().status, TimerStatus::Ready);
    }

    #[test]
    fn stopping_a_pomodoro_early_counts_nothing() {
        let mut t = running(0);
        assert_eq!(t.stop(20 * MIN), None);
        assert_eq!(t.state().status, TimerStatus::Idle);
    }

    #[test]
    fn a_stopwatch_counts_elapsed_time_across_pauses() {
        let mut t = Timer::new();
        t.start(TimerMode::Stopwatch, TimerPreset::default(), 1_000);
        t.pause(1_000 + 5 * MIN);
        t.resume(1_000 + 65 * MIN); // an hour paused is not elapsed
        assert_eq!(t.state().value_ms(1_000 + 70 * MIN), 10 * MIN);
        assert_eq!(
            t.stop(1_000 + 70 * MIN),
            Some(Session {
                kind: SessionKind::Stopwatch,
                secs: 600
            })
        );
    }

    #[test]
    fn a_stopwatch_fumble_is_not_a_session() {
        let mut t = Timer::new();
        t.start(TimerMode::Stopwatch, TimerPreset::default(), 0);
        assert_eq!(t.stop(30_000), None);
    }

    #[test]
    fn a_stopwatch_never_finishes() {
        let mut t = Timer::new();
        t.start(TimerMode::Stopwatch, TimerPreset::default(), 0);
        assert_eq!(t.tick(1000 * MIN), None);
    }

    #[test]
    fn an_absurd_preset_is_clamped() {
        let mut t = Timer::new();
        t.start(
            TimerMode::Pomodoro,
            TimerPreset {
                focus: 0,
                rest: 9999,
            },
            0,
        );
        assert_eq!(t.state().total_ms, MIN);
        assert_eq!(t.state().preset.rest, 60);
    }
}
