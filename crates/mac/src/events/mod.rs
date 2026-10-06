//! System events the notch whispers about: charger and Low Power Mode,
//! external displays, Wi-Fi and VPN. Push, not poll — each domain subscribes to
//! the OS and this module turns what it saw into a stable-id [`Island`].
//!
//! [`start`] installs every source on the *calling thread's* run loop, which
//! must be the main thread: CGDisplay reconfiguration callbacks are delivered
//! through the main run loop (a side thread never sees them), and Tauri's
//! event loop already spins it, so no thread of ours is needed. Callbacks do
//! nothing slow; anything that waits (settling, the battery percent) runs on a
//! short-lived thread.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use aloud_islands::{Island, IslandKind, IslandPriority, IslandSource, IslandTier};

#[cfg(target_os = "macos")]
mod display;
#[cfg(target_os = "macos")]
mod network;
#[cfg(target_os = "macos")]
mod power;

/// Stable ids: the same id updates in place and restarts its clock, so a
/// charger plugged twice in a second is one whisper, not two.
pub const CHARGER_ID: &str = "power:charger";
pub const LOW_POWER_ID: &str = "power:lowpower";
pub const DISPLAY_ID: &str = "display:external";
pub const WIFI_ID: &str = "net:wifi";
pub const VPN_ID: &str = "net:vpn";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The Mac's power source changed. `percent` is the internal battery.
    Charger {
        plugged: bool,
        percent: Option<u8>,
    },
    LowPower(bool),
    /// An external display was connected or removed.
    ExternalDisplay(bool),
    /// No SSID without Location access, so only whether Wi-Fi is up.
    Wifi(bool),
    Vpn(bool),
    /// The power sources changed in any way, including the battery level. Not
    /// a takeover: the app uses it to re-read the battery instead of polling.
    BatteryMoved,
}

impl Event {
    /// The whisper for it; None for [`Event::BatteryMoved`].
    pub fn island(&self) -> Option<Island> {
        let whisper = |id: &str, priority, icon: &str, title: &str, short: &str, source| Island {
            id: id.into(),
            kind: IslandKind::Alert,
            priority,
            tier: Some(IslandTier::Whisper),
            icon: Some(icon.into()),
            title: title.into(),
            short: Some(short.into()),
            source,
            ..Island::default()
        };
        use IslandPriority::{Low, Normal};
        use IslandSource::{Connectivity, Power};
        Some(match *self {
            Event::Charger { plugged, percent } => {
                let pct = percent.map(|p| format!("{p}%"));
                let (icon, title, word) = if plugged {
                    ("bolt", "Charging", "Charging")
                } else {
                    ("battery", "On battery", "Battery")
                };
                let mut i = whisper(
                    CHARGER_ID,
                    Normal,
                    icon,
                    title,
                    pct.as_deref().unwrap_or(word),
                    Power,
                );
                i.detail = pct;
                i
            }
            Event::LowPower(on) => whisper(
                LOW_POWER_ID,
                Normal,
                "battery",
                if on {
                    "Low Power Mode on"
                } else {
                    "Low Power Mode off"
                },
                if on { "Low Power" } else { "Full power" },
                Power,
            ),
            Event::ExternalDisplay(up) => whisper(
                DISPLAY_ID,
                Normal,
                "display",
                if up {
                    "External display connected"
                } else {
                    "External display disconnected"
                },
                if up { "Connected" } else { "Unplugged" },
                Connectivity,
            ),
            // Low: Wi-Fi rejoining is the least newsworthy thing here, so it
            // waits for an idle notch.
            Event::Wifi(up) => whisper(
                WIFI_ID,
                Low,
                "wifi",
                if up { "Wi-Fi connected" } else { "Wi-Fi off" },
                if up { "Wi-Fi on" } else { "Wi-Fi off" },
                Connectivity,
            ),
            Event::Vpn(up) => whisper(
                VPN_ID,
                Normal,
                "lock",
                if up { "VPN on" } else { "VPN off" },
                if up { "VPN on" } else { "VPN off" },
                Connectivity,
            ),
            Event::BatteryMoved => return None,
        })
    }
}

pub type Emit = Arc<dyn Fn(Event) + Send + Sync>;

/// Which sources to install. The example picks; the app takes all.
#[derive(Debug, Clone, Copy)]
pub struct Sources {
    pub power: bool,
    pub display: bool,
    pub network: bool,
}

impl Sources {
    pub const ALL: Self = Self {
        power: true,
        display: true,
        network: true,
    };
}

/// Subscribes to the OS and calls `emit` for each change, from the main run
/// loop or a short-lived thread. Call on the main thread, once. Each source's
/// current state is read first, so nothing is announced for how things already
/// are. Does nothing off macOS.
pub fn start(sources: Sources, emit: Emit) {
    #[cfg(target_os = "macos")]
    {
        if sources.power {
            power::install(emit.clone());
        }
        if sources.display {
            display::install(emit.clone());
        }
        if sources.network {
            network::install(emit);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (sources, emit);
}

/// What each source reads right now, one line each, for the example.
#[cfg(target_os = "macos")]
pub fn snapshot() -> Vec<String> {
    vec![
        format!(
            "power: plugged {}, battery {:?}%, low power {}",
            power::plugged(),
            power::battery_percent(),
            power::low_power_on()
        ),
        format!("display: {} external", display::external_count()),
        format!("network: {}", network::snapshot()),
    ]
}

/// Waits for flapping to stop before reporting a change. A Wi-Fi drop and
/// rejoin inside the delay, a display's several reconfiguration callbacks, a
/// charger wiggled in its socket: each is one report, or none when the value
/// ends where it began.
pub struct Settle<T> {
    delay: Duration,
    inner: Mutex<Inner<T>>,
}

#[derive(Debug)]
struct Inner<T> {
    /// What the app was last told.
    reported: T,
    /// What was last seen.
    latest: T,
    generation: u64,
}

impl<T: Copy + PartialEq + Send + 'static> Settle<T> {
    pub fn new(initial: T, delay: Duration) -> Arc<Self> {
        Arc::new(Self {
            delay,
            inner: Mutex::new(Inner {
                reported: initial,
                latest: initial,
                generation: 0,
            }),
        })
    }

    /// Records a reading. If it differs from the last, `report(from, to)` runs
    /// on a new thread once nothing newer has arrived for the delay and the
    /// value differs from what was last reported.
    pub fn observe(self: &Arc<Self>, now: T, report: impl FnOnce(T, T) + Send + 'static) {
        let Some(ticket) = self.note(now) else { return };
        let this = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(this.delay);
            if let Some((from, to)) = this.settle(ticket) {
                report(from, to);
            }
        });
    }

    fn note(&self, now: T) -> Option<u64> {
        let mut s = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if s.latest == now {
            return None;
        }
        s.latest = now;
        s.generation += 1;
        Some(s.generation)
    }

    fn settle(&self, ticket: u64) -> Option<(T, T)> {
        let mut s = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if s.generation != ticket || s.latest == s.reported {
            return None;
        }
        let (from, to) = (s.reported, s.latest);
        s.reported = to;
        Some((from, to))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle() -> Arc<Settle<bool>> {
        Settle::new(true, Duration::ZERO)
    }

    #[test]
    fn a_steady_reading_reports_nothing() {
        let s = settle();
        assert_eq!(s.note(true), None);
    }

    /// The failure: Wi-Fi drops and rejoins inside the delay and the notch
    /// whispers "Wi-Fi off" then "Wi-Fi connected" for a blip nobody noticed.
    #[test]
    fn a_drop_and_rejoin_inside_the_delay_reports_nothing() {
        let s = settle();
        let drop = s.note(false).unwrap();
        let rejoin = s.note(true).unwrap();
        assert_eq!(s.settle(drop), None, "superseded by the rejoin");
        assert_eq!(s.settle(rejoin), None, "back where it was reported");
    }

    #[test]
    fn a_lasting_change_reports_once_with_both_values() {
        let s = settle();
        let a = s.note(false).unwrap();
        assert_eq!(s.settle(a), Some((true, false)));
        assert_eq!(s.settle(a), None, "not twice");
        let b = s.note(true).unwrap();
        assert_eq!(s.settle(b), Some((false, true)));
    }

    /// A display reconfiguration is several callbacks: only the last reading
    /// counts, and the report spans the whole change.
    #[test]
    fn several_callbacks_for_one_change_report_the_last() {
        let s = Settle::new(1u32, Duration::ZERO);
        let t1 = s.note(0).unwrap();
        let t2 = s.note(2).unwrap();
        assert_eq!(s.settle(t1), None);
        assert_eq!(s.settle(t2), Some((1, 2)));
    }

    #[test]
    fn observe_reports_from_a_thread() {
        let s = settle();
        let (tx, rx) = std::sync::mpsc::channel();
        s.observe(false, move |from, to| tx.send((from, to)).unwrap());
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            (true, false)
        );
    }

    #[test]
    fn every_event_but_battery_moved_is_a_whisper_with_a_stable_id() {
        for (e, id, glyph) in [
            (
                Event::Charger {
                    plugged: true,
                    percent: Some(82),
                },
                CHARGER_ID,
                "bolt",
            ),
            (Event::LowPower(true), LOW_POWER_ID, "battery"),
            (Event::ExternalDisplay(true), DISPLAY_ID, "display"),
            (Event::Wifi(false), WIFI_ID, "wifi"),
            (Event::Vpn(true), VPN_ID, "lock"),
        ] {
            let i = e.island().unwrap();
            assert_eq!(
                (i.id.as_str(), i.icon.as_deref(), i.tier),
                (id, Some(glyph), Some(IslandTier::Whisper))
            );
            assert_eq!(i.kind, IslandKind::Alert);
        }
        assert!(Event::BatteryMoved.island().is_none());
    }

    #[test]
    fn the_charger_whisper_carries_the_percent_when_known() {
        let on = |plugged, percent| {
            Event::Charger { plugged, percent }
                .island()
                .unwrap()
                .short
                .unwrap()
        };
        assert_eq!(on(true, Some(82)), "82%");
        assert_eq!(on(true, None), "Charging");
        assert_eq!(on(false, None), "Battery");
    }
}
