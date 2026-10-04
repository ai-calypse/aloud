//! What the timer and keep-awake put in the closed notch. Pure: state and a
//! clock in, an [`Island`] out, so the wording is tested rather than eyeballed.

use aloud_islands::{Island, IslandAction, IslandKind, IslandPriority};

use crate::awake::AwakeState;
use crate::timer::{Finished, TimerMode, TimerPhase, TimerState, TimerStatus};

pub const TIMER_ID: &str = "timer";
pub const AWAKE_ID: &str = "awake";

/// `18:42`, or `1:05:00` past the hour. A countdown rounds *up*, so the face
/// reads 0:01 until the last second is gone rather than 0:00 a second early.
pub fn clock(ms: i64, countdown: bool) -> String {
    let ms = ms.max(0);
    let secs = if countdown {
        (ms + 999) / 1000
    } else {
        ms / 1000
    };
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn title(s: &TimerState) -> String {
    match (s.mode, s.phase) {
        (TimerMode::Stopwatch, _) => "Stopwatch".into(),
        (_, TimerPhase::Focus) => format!("Focus {} of {}", s.round, s.rounds),
        (_, TimerPhase::ShortBreak) => "Short break".into(),
        (_, TimerPhase::LongBreak) => "Long break".into(),
    }
}

fn rest_word(phase: TimerPhase) -> &'static str {
    match phase {
        TimerPhase::Focus => "focus",
        TimerPhase::ShortBreak => "break",
        TimerPhase::LongBreak => "long break",
    }
}

/// The live island for the timer, None while idle.
pub fn timer(s: &TimerState, now: i64) -> Option<Island> {
    let countdown = s.mode == TimerMode::Pomodoro;
    let face = clock(s.value_ms(now), countdown);
    let (detail, action, priority) = match s.status {
        TimerStatus::Idle => return None,
        TimerStatus::Running => (
            face,
            IslandAction::new("Pause", "focus.pause"),
            IslandPriority::High,
        ),
        TimerStatus::Paused => (
            format!("{face} paused"),
            IslandAction::new("Resume", "focus.resume"),
            IslandPriority::Normal,
        ),
        TimerStatus::Ready => (
            format!("{} ready", rest_word(s.phase)),
            IslandAction::new("Start", "focus.resume"),
            IslandPriority::High,
        ),
    };
    Some(Island {
        id: TIMER_ID.into(),
        kind: IslandKind::Live,
        priority,
        icon: Some("timer".into()),
        title: title(s),
        detail: Some(detail),
        actions: vec![action],
        expires: None,
        dismissible: false,
    })
}

/// The chip that drops when a phase ends.
pub fn finished(f: &Finished) -> Island {
    let (title, detail) = match f.phase {
        TimerPhase::Focus => ("Focus done", format!("{} next", rest_word(f.next))),
        _ => ("Break over", "ready to focus".into()),
    };
    Island {
        id: "timer:done".into(),
        kind: IslandKind::Alert,
        priority: IslandPriority::High,
        icon: Some("timer".into()),
        title: title.into(),
        detail: Some(detail),
        actions: vec![IslandAction::new("Start", "focus.resume")],
        expires: Some(8_000),
        dismissible: false,
    }
}

/// The keep-awake island, None when off. Minute resolution: it is re-pushed
/// every few seconds and an identical island sends nothing.
pub fn awake(s: &AwakeState, now: i64) -> Option<Island> {
    if !s.active {
        return None;
    }
    let detail = match s.until {
        None => "Always".to_string(),
        Some(until) => left((until - now).max(0)),
    };
    Some(Island {
        id: AWAKE_ID.into(),
        kind: IslandKind::Live,
        priority: IslandPriority::Low,
        icon: Some("awake".into()),
        title: "Keeping your Mac awake".into(),
        detail: Some(detail),
        actions: vec![IslandAction::new("Stop", "focus.awakeOff")],
        expires: None,
        dismissible: false,
    })
}

/// `1h 42m left`, `12m left`, `<1m left`: whole minutes, rounded up.
fn left(ms: i64) -> String {
    let mins = (ms + 59_999) / 60_000;
    match mins {
        0 => "ending".into(),
        1..=59 => format!("{mins}m left"),
        _ => format!("{}h {:02}m left", mins / 60, mins % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::{Timer, TimerPreset};

    #[test]
    fn a_countdown_shows_the_last_second_not_zero() {
        assert_eq!(clock(25 * 60_000, true), "25:00");
        assert_eq!(clock(1, true), "0:01");
        assert_eq!(clock(0, true), "0:00");
        assert_eq!(clock(61 * 60_000, true), "1:01:00");
        // A stopwatch counts up from whole seconds.
        assert_eq!(clock(59_999, false), "0:59");
    }

    #[test]
    fn the_island_follows_the_timer_and_names_what_a_tap_does() {
        let mut t = Timer::new();
        assert!(timer(&t.state(), 0).is_none());
        t.start(TimerMode::Pomodoro, TimerPreset::default(), 0);
        let i = timer(&t.state(), 6 * 60_000 + 18_000).unwrap();
        assert_eq!(i.title, "Focus 1 of 4");
        assert_eq!(i.detail.as_deref(), Some("18:42"));
        assert_eq!(i.actions[0].action, "focus.pause");
        t.pause(6 * 60_000 + 18_000);
        let i = timer(&t.state(), 9_999_999).unwrap();
        assert_eq!(i.detail.as_deref(), Some("18:42 paused"));
        assert_eq!(i.actions[0].action, "focus.resume");
        assert_eq!(i.priority, IslandPriority::Normal);
    }

    #[test]
    fn a_waiting_phase_asks_to_be_started() {
        let mut t = Timer::new();
        t.start(TimerMode::Pomodoro, TimerPreset::default(), 0);
        let done = t.tick(25 * 60_000).unwrap();
        let live = timer(&t.state(), 25 * 60_000).unwrap();
        assert_eq!(live.detail.as_deref(), Some("break ready"));
        assert_eq!(finished(&done).detail.as_deref(), Some("break next"));
    }

    #[test]
    fn awake_counts_whole_minutes_up() {
        let on = |until| AwakeState {
            active: true,
            until,
            minutes: None,
        };
        assert_eq!(
            awake(&on(Some(5_400_000)), 0).unwrap().detail.as_deref(),
            Some("1h 30m left")
        );
        assert_eq!(
            awake(&on(Some(30_001)), 0).unwrap().detail.as_deref(),
            Some("1m left")
        );
        assert_eq!(
            awake(&on(None), 0).unwrap().detail.as_deref(),
            Some("Always")
        );
        assert!(awake(&AwakeState::default(), 0).is_none());
    }
}
