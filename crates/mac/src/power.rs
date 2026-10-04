//! Battery state for Mac › Power, and what it raises: a chip at 20%, a pinned
//! dismissible island below 10%, and a chip when fully charged.
//!
//! `pmset -g batt` is the charge, state and time estimate — the same numbers
//! as the menu bar, which `ioreg`'s own `TimeRemaining` is not (it read 178
//! minutes where pmset said 7:20). `ioreg` supplies what pmset has no word
//! for: cycle count and capacity health.

use aloud_islands::{Island, IslandAction, IslandKind, IslandPriority};
use serde::{Deserialize, Serialize};

use crate::Step;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChargeState {
    Charging,
    Discharging,
    /// On the charger and at 100%.
    Full,
    /// On the charger but not charging: optimised charging holding at 80%.
    PluggedIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub percent: u8,
    pub state: ChargeState,
    /// To empty while discharging, to full while charging; None while macOS
    /// is still estimating.
    pub minutes: Option<u32>,
    pub cycles: Option<u32>,
    /// Capacity now over capacity when new, 0–100.
    pub health: Option<u8>,
}

/// Reads the internal battery; None on a Mac without one.
#[cfg(target_os = "macos")]
pub fn read() -> Option<Battery> {
    let pmset = run("pmset", &["-g", "batt"])?;
    let mut battery = parse_pmset(&pmset)?;
    if let Some(ioreg) = run("ioreg", &["-rn", "AppleSmartBattery"]) {
        let (cycles, health) = parse_ioreg(&ioreg);
        battery.cycles = cycles;
        battery.health = health;
    }
    Some(battery)
}

#[cfg(not(target_os = "macos"))]
pub fn read() -> Option<Battery> {
    None
}

#[cfg(target_os = "macos")]
fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `-InternalBattery-0 (id=1)\t71%; discharging; 7:20 remaining present: true`.
pub fn parse_pmset(text: &str) -> Option<Battery> {
    let line = text.lines().find(|l| l.contains("InternalBattery"))?;
    let detail = line.split_once('\t').map_or(line, |(_, rest)| rest);
    let mut parts = detail.split(';').map(str::trim);
    let percent: u8 = parts.next()?.strip_suffix('%')?.parse().ok()?;
    let state_word = parts.next()?;
    let rest = parts.next().unwrap_or_default();
    // Whole-word match: "discharging" contains "charging".
    let state = match state_word {
        "discharging" => ChargeState::Discharging,
        "charged" => ChargeState::Full,
        "charging" | "finishing charge" => ChargeState::Charging,
        // "AC attached", with "not charging" following.
        _ => ChargeState::PluggedIn,
    };
    // A full pack reads "0:00 remaining", which is no estimate.
    let minutes = rest
        .split_whitespace()
        .next()
        .and_then(|t| t.split_once(':'))
        .and_then(|(h, m)| Some(h.parse::<u32>().ok()? * 60 + m.parse::<u32>().ok()?))
        .filter(|m| *m > 0);
    // 100% on the charger is Full whatever pmset calls the state.
    let state = if percent >= 100 && state != ChargeState::Discharging {
        ChargeState::Full
    } else {
        state
    };
    Some(Battery {
        percent,
        state,
        minutes,
        cycles: None,
        health: None,
    })
}

/// Cycle count and health from `ioreg -rn AppleSmartBattery`. Health is the
/// raw max capacity over the design capacity: `MaxCapacity` is a percentage
/// on Apple silicon and says 100 for every pack.
pub fn parse_ioreg(text: &str) -> (Option<u32>, Option<u8>) {
    let value = |key: &str| -> Option<u32> {
        let needle = format!("\"{key}\" = ");
        text.lines()
            .find_map(|l| l.trim().strip_prefix(&needle))
            .and_then(|v| v.trim().parse().ok())
    };
    let health = match (value("AppleRawMaxCapacity"), value("DesignCapacity")) {
        (Some(now), Some(design)) if design > 0 => Some(
            (f64::from(now) / f64::from(design) * 100.0)
                .round()
                .min(100.0) as u8,
        ),
        _ => None,
    };
    (value("CycleCount"), health)
}

/// At or below this on battery the notch drops a chip, like the system alert.
pub const LOW: u8 = 20;
/// At or below this it stays up beside the camera: the machine is about to
/// sleep.
pub const CRITICAL: u8 = 10;

/// The pinned island, only while critical.
pub const LOW_ID: &str = "mac:battery";
pub const LOW_CHIP_ID: &str = "mac:battery-low";
pub const FULL_ID: &str = "mac:battery-full";

fn open_power() -> IslandAction {
    IslandAction::new("Show", "mac.power")
}

/// The critical-battery island: live until the Mac is plugged in or the user
/// dismisses it, so it is the one thing here that can sit beside the camera.
fn critical_island(b: &Battery) -> Island {
    Island {
        id: LOW_ID.into(),
        kind: IslandKind::Live,
        priority: IslandPriority::Urgent,
        icon: Some("battery".into()),
        title: "Battery low".into(),
        detail: Some(format!("{}%", b.percent)),
        actions: vec![open_power()],
        expires: None,
        dismissible: true,
    }
}

/// The chip at 20%. An alert, so it drops once from the notch and goes; the
/// pinned island above is for when it is truly urgent.
fn low_chip(b: &Battery) -> Island {
    Island {
        id: LOW_CHIP_ID.into(),
        kind: IslandKind::Alert,
        priority: IslandPriority::Normal,
        icon: Some("battery".into()),
        title: "Battery low".into(),
        detail: Some(format!("{}%", b.percent)),
        actions: vec![open_power()],
        // The webview ends an alert after its own four seconds; this is only
        // the backstop (see agent.rs ALERT_BACKSTOP_MS).
        expires: Some(30_000),
        dismissible: false,
    }
}

/// The full-charge chip. An alert, so it drops once from the notch and goes.
fn full_island() -> Island {
    Island {
        id: FULL_ID.into(),
        kind: IslandKind::Alert,
        priority: IslandPriority::Normal,
        icon: Some("battery".into()),
        title: "Fully charged".into(),
        detail: Some("100%".into()),
        actions: vec![open_power()],
        expires: Some(30_000),
        dismissible: false,
    }
}

/// Decides which islands the battery wants. Both chips fire on a
/// *transition* — into full, into low — so a Mac sitting at 100% or at 15%
/// for a day does not chirp every poll. Launching already low is a
/// transition from "unknown", so it still tells the user once.
#[derive(Debug, Default)]
pub struct Watch {
    was_full: Option<bool>,
    /// Discharging at or below [`LOW`] on the last read: the chip is spent
    /// until the battery recovers or is plugged in.
    was_low: bool,
    /// The user closed the critical island. The watcher re-pushes every few
    /// polls, so it is remembered here until the battery recovers.
    dismissed: bool,
}

impl Watch {
    pub const fn new() -> Self {
        Self {
            was_full: None,
            was_low: false,
            dismissed: false,
        }
    }

    /// The user dismissed the pinned island (`dismiss_island` for [`LOW_ID`]).
    pub fn dismiss(&mut self) {
        self.dismissed = true;
    }

    pub fn update(&mut self, battery: Option<&Battery>) -> Vec<Step> {
        let Some(b) = battery else {
            return vec![Step::Remove(LOW_ID)];
        };
        let full = b.state == ChargeState::Full;
        let just_filled = full && self.was_full == Some(false);
        self.was_full = Some(full);

        let discharging = b.state == ChargeState::Discharging;
        let low = discharging && b.percent <= LOW;
        let just_low = low && !self.was_low;
        self.was_low = low;
        let critical = discharging && b.percent <= CRITICAL;
        if !critical {
            self.dismissed = false;
        }

        let mut steps = Vec::new();
        if critical && !self.dismissed {
            steps.push(Step::Push(critical_island(b)));
        } else {
            steps.push(Step::Remove(LOW_ID));
        }
        if just_low {
            steps.push(Step::Push(low_chip(b)));
        }
        if just_filled {
            steps.push(Step::Push(full_island()));
        }
        steps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn battery(percent: u8, state: ChargeState) -> Battery {
        Battery {
            percent,
            state,
            minutes: None,
            cycles: None,
            health: None,
        }
    }

    #[test]
    fn pmset_lines_parse_into_state_and_time() {
        let on_battery = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=22675555)\t71%; discharging; 7:20 remaining present: true\n";
        let b = parse_pmset(on_battery).unwrap();
        assert_eq!(
            (b.percent, b.state, b.minutes),
            (71, ChargeState::Discharging, Some(440))
        );

        let charging = " -InternalBattery-0 (id=1)\t80%; charging; 0:45 remaining present: true";
        let b = parse_pmset(charging).unwrap();
        assert_eq!((b.state, b.minutes), (ChargeState::Charging, Some(45)));

        let estimating =
            " -InternalBattery-0 (id=1)\t52%; discharging; (no estimate) present: true";
        assert_eq!(parse_pmset(estimating).unwrap().minutes, None);

        let held = " -InternalBattery-0 (id=1)\t80%; AC attached; not charging present: true";
        assert_eq!(parse_pmset(held).unwrap().state, ChargeState::PluggedIn);

        let full = " -InternalBattery-0 (id=1)\t100%; charged; 0:00 remaining present: true";
        let b = parse_pmset(full).unwrap();
        assert_eq!((b.state, b.minutes), (ChargeState::Full, None));
    }

    #[test]
    fn a_desktop_mac_has_no_battery() {
        assert_eq!(parse_pmset("Now drawing from 'AC Power'\n"), None);
    }

    #[test]
    fn health_is_raw_capacity_over_design_not_the_percentage_field() {
        let ioreg = "    \"MaxCapacity\" = 100\n    \"DesignCapacity\" = 4629\n    \"CycleCount\" = 112\n    \"AppleRawMaxCapacity\" = 4439\n";
        assert_eq!(parse_ioreg(ioreg), (Some(112), Some(96)));
        assert_eq!(parse_ioreg("nothing here"), (None, None));
    }

    fn pushes(steps: &[Step], id: &str) -> bool {
        steps
            .iter()
            .any(|s| matches!(s, Step::Push(i) if i.id == id))
    }

    /// The failure: a "Battery low" island pinned beside the camera from 20%
    /// down, forever.
    #[test]
    fn twenty_percent_is_a_chip_and_only_critical_is_pinned() {
        let mut watch = Watch::default();
        watch.update(Some(&battery(50, ChargeState::Discharging)));
        let steps = watch.update(Some(&battery(20, ChargeState::Discharging)));
        assert!(pushes(&steps, LOW_CHIP_ID));
        assert!(!pushes(&steps, LOW_ID), "not pinned above 10%");
        assert!(matches!(&steps[..], [Step::Remove(LOW_ID), Step::Push(i)]
                if i.kind == IslandKind::Alert));

        let steps = watch.update(Some(&battery(10, ChargeState::Discharging)));
        assert!(matches!(&steps[0], Step::Push(i)
            if i.id == LOW_ID && i.kind == IslandKind::Live
                && i.priority == IslandPriority::Urgent && i.dismissible));
        assert!(!pushes(&steps, LOW_CHIP_ID), "the chip is not repeated");
    }

    #[test]
    fn the_low_chip_does_not_repeat_on_every_poll() {
        let mut watch = Watch::default();
        watch.update(Some(&battery(30, ChargeState::Discharging)));
        assert!(pushes(
            &watch.update(Some(&battery(20, ChargeState::Discharging))),
            LOW_CHIP_ID
        ));
        for percent in [19, 18, 15, 12] {
            assert!(!pushes(
                &watch.update(Some(&battery(percent, ChargeState::Discharging))),
                LOW_CHIP_ID
            ));
        }
    }

    #[test]
    fn launching_already_low_raises_the_chip_once() {
        let mut watch = Watch::default();
        assert!(pushes(
            &watch.update(Some(&battery(14, ChargeState::Discharging))),
            LOW_CHIP_ID
        ));
        assert!(!pushes(
            &watch.update(Some(&battery(14, ChargeState::Discharging))),
            LOW_CHIP_ID
        ));
    }

    /// Plugging in, or recovering above 20, re-arms the chip for the next fall.
    #[test]
    fn the_chip_rearms_after_a_charge_or_a_recovery() {
        let mut watch = Watch::default();
        watch.update(Some(&battery(19, ChargeState::Discharging)));
        // Plugged in at the same 19%.
        let steps = watch.update(Some(&battery(19, ChargeState::Charging)));
        assert_eq!(steps, vec![Step::Remove(LOW_ID)]);
        assert!(pushes(
            &watch.update(Some(&battery(19, ChargeState::Discharging))),
            LOW_CHIP_ID
        ));
        // Recovered above 20 while still reading as discharging.
        watch.update(Some(&battery(25, ChargeState::Discharging)));
        assert!(pushes(
            &watch.update(Some(&battery(20, ChargeState::Discharging))),
            LOW_CHIP_ID
        ));
    }

    /// The failure: the watcher re-pushing the island the user just closed
    /// every ~30 seconds.
    #[test]
    fn a_dismissed_critical_island_stays_gone_until_the_battery_recovers() {
        let mut watch = Watch::default();
        assert!(pushes(
            &watch.update(Some(&battery(8, ChargeState::Discharging))),
            LOW_ID
        ));
        watch.dismiss();
        for percent in [8, 7, 6] {
            let steps = watch.update(Some(&battery(percent, ChargeState::Discharging)));
            assert!(!pushes(&steps, LOW_ID), "dismissed at {percent}%");
        }
        // Plugged in: the memory clears, so the next fall raises it again.
        watch.update(Some(&battery(6, ChargeState::Charging)));
        assert!(pushes(
            &watch.update(Some(&battery(9, ChargeState::Discharging))),
            LOW_ID
        ));

        // Recovery by percentage alone clears it too.
        watch.dismiss();
        watch.update(Some(&battery(11, ChargeState::Discharging)));
        assert!(pushes(
            &watch.update(Some(&battery(10, ChargeState::Discharging))),
            LOW_ID
        ));
    }

    #[test]
    fn no_battery_takes_the_pinned_island_down() {
        let mut watch = Watch::default();
        watch.update(Some(&battery(5, ChargeState::Discharging)));
        assert_eq!(watch.update(None), vec![Step::Remove(LOW_ID)]);
    }

    /// The failure: chirping "Fully charged" at launch, and again on every
    /// poll of a Mac left on the charger.
    #[test]
    fn full_charge_fires_on_the_transition_only() {
        let mut watch = Watch::default();
        let full = battery(100, ChargeState::Full);
        let announced = |steps: &[Step]| {
            steps
                .iter()
                .any(|s| matches!(s, Step::Push(i) if i.id == FULL_ID))
        };

        assert!(
            !announced(&watch.update(Some(&full))),
            "already full at launch"
        );
        assert!(!announced(&watch.update(Some(&full))));
        assert!(!announced(
            &watch.update(Some(&battery(99, ChargeState::Charging)))
        ));
        assert!(
            announced(&watch.update(Some(&full))),
            "filled up while watching"
        );
        assert!(!announced(&watch.update(Some(&full))), "and only once");
    }
}
