//! Battery state for Mac › Power, and the two islands it can raise: low, and
//! fully charged.
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
    let out = std::process::Command::new(program).args(args).output().ok()?;
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
        (Some(now), Some(design)) if design > 0 => {
            Some((f64::from(now) / f64::from(design) * 100.0).round().min(100.0) as u8)
        }
        _ => None,
    };
    (value("CycleCount"), health)
}

/// At or below this on battery the notch says so, like the system alert does.
pub const LOW: u8 = 20;
/// Below this it is urgent: the machine is about to sleep.
pub const CRITICAL: u8 = 10;

pub const LOW_ID: &str = "mac:battery";
pub const FULL_ID: &str = "mac:battery-full";

fn open_power() -> IslandAction {
    IslandAction::new("Show", "mac.power")
}

/// The low-battery island: live while it lasts, so it rides beside the camera
/// until the Mac is plugged in.
fn low_island(b: &Battery) -> Island {
    Island {
        id: LOW_ID.into(),
        kind: IslandKind::Live,
        priority: if b.percent <= CRITICAL {
            IslandPriority::Urgent
        } else {
            IslandPriority::High
        },
        icon: Some("battery".into()),
        title: "Battery low".into(),
        detail: Some(format!("{}%", b.percent)),
        actions: vec![open_power()],
        expires: None,
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
        // The webview ends an alert after its own four seconds; this is only
        // the backstop (see agent.rs ALERT_BACKSTOP_MS).
        expires: Some(30_000),
    }
}

/// Decides which islands the battery wants. The full chip fires on the
/// *transition* to full, so launching Aloud on an already-full Mac is silent
/// and a Mac sitting at 100% for a day does not chirp every poll.
#[derive(Debug, Default)]
pub struct Watch {
    was_full: Option<bool>,
}

impl Watch {
    pub fn update(&mut self, battery: Option<&Battery>) -> Vec<Step> {
        let Some(b) = battery else {
            return vec![Step::Remove(LOW_ID)];
        };
        let full = b.state == ChargeState::Full;
        let just_filled = full && self.was_full == Some(false);
        self.was_full = Some(full);

        let mut steps = Vec::new();
        if b.state == ChargeState::Discharging && b.percent <= LOW {
            steps.push(Step::Push(low_island(b)));
        } else {
            steps.push(Step::Remove(LOW_ID));
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
        assert_eq!((b.percent, b.state, b.minutes), (71, ChargeState::Discharging, Some(440)));

        let charging = " -InternalBattery-0 (id=1)\t80%; charging; 0:45 remaining present: true";
        let b = parse_pmset(charging).unwrap();
        assert_eq!((b.state, b.minutes), (ChargeState::Charging, Some(45)));

        let estimating = " -InternalBattery-0 (id=1)\t52%; discharging; (no estimate) present: true";
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

    #[test]
    fn low_battery_is_live_only_while_unplugged() {
        let mut watch = Watch::default();
        let steps = watch.update(Some(&battery(18, ChargeState::Discharging)));
        assert!(matches!(&steps[0], Step::Push(i) if i.id == LOW_ID && i.priority == IslandPriority::High));
        // Plugged in at the same 18%: the island goes.
        assert_eq!(
            watch.update(Some(&battery(18, ChargeState::Charging))),
            vec![Step::Remove(LOW_ID)]
        );
        let steps = watch.update(Some(&battery(8, ChargeState::Discharging)));
        assert!(matches!(&steps[0], Step::Push(i) if i.priority == IslandPriority::Urgent));
        assert_eq!(watch.update(None), vec![Step::Remove(LOW_ID)]);
    }

    /// The failure: chirping "Fully charged" at launch, and again on every
    /// poll of a Mac left on the charger.
    #[test]
    fn full_charge_fires_on_the_transition_only() {
        let mut watch = Watch::default();
        let full = battery(100, ChargeState::Full);
        let announced = |steps: &[Step]| steps.iter().any(|s| matches!(s, Step::Push(i) if i.id == FULL_ID));

        assert!(!announced(&watch.update(Some(&full))), "already full at launch");
        assert!(!announced(&watch.update(Some(&full))));
        assert!(!announced(&watch.update(Some(&battery(99, ChargeState::Charging)))));
        assert!(announced(&watch.update(Some(&full))), "filled up while watching");
        assert!(!announced(&watch.update(Some(&full))), "and only once");
    }
}
