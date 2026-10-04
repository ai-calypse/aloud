//! CPU, memory, disk, network and thermal pressure, for Mac › Stats.
//!
//! Temperatures are the public `NSProcessInfo.thermalState` (nominal → critical),
//! not degrees: numeric sensor values are SMC keys Apple does not document and
//! changes per chip generation.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sysinfo::{Disks, Networks, System};

/// macOS needs a gap between two CPU reads to compute a load; below this the
/// number is noise.
const MIN_CPU_INTERVAL: Duration = Duration::from_millis(250);
/// Past this the previous read is too old to be "now", so a fresh sample takes
/// its own short baseline instead of averaging over however long the panel was shut.
const STALE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Thermal {
    Nominal,
    Fair,
    Serious,
    Critical,
}

/// One reading. Floats throughout: specta refuses 64-bit integers, and a
/// byte count shown as "11.2 GB" does not need exactness.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SystemStats {
    /// 0–100, all cores.
    pub cpu: f64,
    pub memory_used_gb: f64,
    pub memory_total_gb: f64,
    pub disk_used_gb: f64,
    pub disk_total_gb: f64,
    /// Bytes per second since the previous read, all non-loopback interfaces.
    pub net_down: f64,
    pub net_up: f64,
    pub thermal: Thermal,
}

pub struct Sampler {
    system: System,
    disks: Disks,
    networks: Networks,
    last: Instant,
}

const GB: f64 = 1_073_741_824.0;

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_usage();
        Self {
            system,
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            last: Instant::now(),
        }
    }

    /// Reads everything. Blocks for a quarter second when the last read is
    /// stale or too recent to difference against — call it off the UI thread.
    pub fn sample(&mut self) -> SystemStats {
        let age = self.last.elapsed();
        if age > STALE {
            self.system.refresh_cpu_usage();
            self.networks.refresh(true);
            // The baseline is the clock now: the rates below divide by the
            // quarter second just slept, not by the stale gap.
            self.last = Instant::now();
            std::thread::sleep(MIN_CPU_INTERVAL);
        } else if age < MIN_CPU_INTERVAL {
            std::thread::sleep(MIN_CPU_INTERVAL - age);
        }
        let elapsed = self.last.elapsed();
        self.last = Instant::now();

        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.disks.refresh(true);
        self.networks.refresh(true);

        let (down, up) = self
            .networks
            .iter()
            .filter(|(name, _)| !name.starts_with("lo"))
            .fold((0u64, 0u64), |(d, u), (_, n)| {
                (d + n.received(), u + n.transmitted())
            });
        let (disk_used, disk_total) = disk_usage(
            self.disks
                .iter()
                .map(|d| (d.mount_point().to_string_lossy().into_owned(), d.total_space(), d.available_space())),
        );
        SystemStats {
            cpu: f64::from(self.system.global_cpu_usage()).clamp(0.0, 100.0),
            memory_used_gb: self.system.used_memory() as f64 / GB,
            memory_total_gb: self.system.total_memory() as f64 / GB,
            disk_used_gb: disk_used as f64 / GB,
            disk_total_gb: disk_total as f64 / GB,
            net_down: rate(down, elapsed),
            net_up: rate(up, elapsed),
            thermal: thermal(),
        }
    }
}

/// Bytes over a span as a per-second rate. Zero span is zero, not infinity.
fn rate(bytes: u64, over: Duration) -> f64 {
    let secs = over.as_secs_f64();
    if secs <= 0.0 {
        0.0
    } else {
        bytes as f64 / secs
    }
}

/// Used and total bytes of the startup volume. On APFS every volume of a
/// container reports the container's free space, so `/` answers for the
/// user's data too; with no `/` (not macOS) the largest disk stands in.
fn disk_usage(disks: impl Iterator<Item = (String, u64, u64)>) -> (u64, u64) {
    let disks: Vec<_> = disks.collect();
    let pick = disks
        .iter()
        .find(|(mount, ..)| mount == "/")
        .or_else(|| disks.iter().max_by_key(|(_, total, _)| *total));
    pick.map_or((0, 0), |(_, total, free)| (total.saturating_sub(*free), *total))
}

#[cfg(target_os = "macos")]
fn thermal() -> Thermal {
    use objc2_foundation::{NSProcessInfo, NSProcessInfoThermalState};
    // SAFETY: a read-only property of the shared process-info object, which is thread-safe.
    match unsafe { NSProcessInfo::processInfo().thermalState() } {
        NSProcessInfoThermalState::Fair => Thermal::Fair,
        NSProcessInfoThermalState::Serious => Thermal::Serious,
        NSProcessInfoThermalState::Critical => Thermal::Critical,
        _ => Thermal::Nominal,
    }
}

#[cfg(not(target_os = "macos"))]
fn thermal() -> Thermal {
    Thermal::Nominal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_is_bytes_per_second_not_per_sample() {
        assert_eq!(rate(3_000, Duration::from_millis(1_500)), 2_000.0);
        assert_eq!(rate(10, Duration::ZERO), 0.0);
    }

    /// The failure: picking the first disk listed, which on a Mac is often a
    /// 500 MB recovery or VM volume, so the bar read "98% full".
    #[test]
    fn the_startup_volume_answers_for_the_disk_not_whichever_is_listed_first() {
        let disks = vec![
            ("/System/Volumes/VM".to_string(), 500, 100),
            ("/".to_string(), 1_000, 250),
        ];
        assert_eq!(disk_usage(disks.into_iter()), (750, 1_000));
        let no_root = vec![("/a".to_string(), 100, 50), ("/b".to_string(), 900, 100)];
        assert_eq!(disk_usage(no_root.into_iter()), (800, 900));
        assert_eq!(disk_usage(std::iter::empty()), (0, 0));
    }
}
