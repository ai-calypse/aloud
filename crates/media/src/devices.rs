//! Bluetooth gear and its batteries: AirPods (left, right, case), headphones,
//! and the Magic Mouse/Keyboard/Trackpad.
//!
//! Two sources, merged: `system_profiler SPBluetoothDataType -json` knows every
//! paired device and the batteries it reports (AirPods, most headphones);
//! `ioreg` knows the Apple HID accessories' `BatteryPercent`.
//!
//! Connect and disconnect edges are not polled for: [`watch`] subscribes to
//! IOBluetooth's notifications and [`react`] turns an edge into notch
//! takeovers. The profiler is read once per connect (batteries appear a moment
//! after one) and whenever a caller already has a fresh list ([`check_batteries`]).
//!
//! ponytail: no "AirPods case nearby" popup. That is BLE scanning of Apple's
//! proximity packets, a Swift helper's job, not a subprocess's.

use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use aloud_islands::{Island, IslandKind, IslandPriority, IslandSource, IslandTier};
use aloud_native::run_jxa;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceBattery {
    /// "Left", "Right", "Case", or "" for a single battery.
    pub label: String,
    pub percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BtDevice {
    pub name: String,
    /// `aa:bb:cc:dd:ee:ff`, lowercase: what connecting names it by.
    pub address: String,
    /// `headphones`, `mouse`, `keyboard`, `trackpad` or `other`.
    pub kind: String,
    pub connected: bool,
    /// The raw parts, as the device reported them.
    pub batteries: Vec<DeviceBattery>,
    /// The headline charge, never the case: the mean of left and right (the
    /// buds are worn as a pair), the one bud alone when only one reports, or
    /// the device's single battery. `None` when only a case reported.
    pub level: Option<u8>,
    /// The charging case, shown small beside the headline.
    pub case: Option<u8>,
    /// "Left" or "Right" when `level` is that bud alone (the other is out of
    /// range or in the case), so the card can say so.
    pub single_bud: Option<String>,
}

impl BtDevice {
    /// Derives `level`, `case` and `single_bud` from `batteries`. Run it
    /// wherever the parts change, so no consumer averages for itself.
    fn settle(mut self) -> Self {
        let of = |label: &str| {
            self.batteries
                .iter()
                .find(|b| b.label == label)
                .map(|b| u16::from(b.percent))
        };
        let (left, right) = (of("Left"), of("Right"));
        self.case = of("Case").map(|p| p as u8);
        self.single_bud = match (left, right) {
            (Some(_), None) => Some("Left".into()),
            (None, Some(_)) => Some("Right".into()),
            _ => None,
        };
        // Rounded half up: two buds at 99 and 100 read 100, not 99.
        self.level = match (left, right) {
            (Some(l), Some(r)) => Some((l + r).div_ceil(2) as u8),
            (Some(p), None) | (None, Some(p)) => Some(p as u8),
            // `of("")` is the single-battery device (mouse, headset).
            (None, None) => of("").map(|p| p as u8),
        };
        self
    }
}

/// Every paired device, connected first, then by name. Blocking: the profiler
/// takes a second or two.
pub fn list() -> Result<Vec<BtDevice>> {
    let profiler = Command::new("system_profiler")
        .args(["SPBluetoothDataType", "-json"])
        .output()
        .context("running system_profiler")?;
    let mut devices = parse_profiler(&String::from_utf8_lossy(&profiler.stdout));
    // ioreg failing only costs the HID batteries.
    if let Ok(out) = Command::new("ioreg")
        .args(["-r", "-k", "BatteryPercent", "-l"])
        .output()
    {
        merge_hid(
            &mut devices,
            parse_ioreg(&String::from_utf8_lossy(&out.stdout)),
        );
    }
    devices.sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.cmp(&b.name)));
    Ok(devices)
}

fn kind_of(minor: &str, name: &str) -> &'static str {
    let probe = format!("{minor} {name}").to_lowercase();
    if probe.contains("mouse") {
        "mouse"
    } else if probe.contains("keyboard") {
        "keyboard"
    } else if probe.contains("trackpad") {
        "trackpad"
    } else if ["headphone", "headset", "airpods", "buds"]
        .iter()
        .any(|k| probe.contains(k))
    {
        "headphones"
    } else {
        "other"
    }
}

/// "85%" as 85.
fn percent(v: &Value) -> Option<u8> {
    v.as_str()?
        .trim()
        .trim_end_matches('%')
        .parse::<u8>()
        .ok()
        .filter(|p| *p <= 100)
}

fn normalize_address(a: &str) -> String {
    a.trim().to_lowercase().replace('-', ":")
}

fn parse_profiler(json: &str) -> Vec<BtDevice> {
    let Ok(root) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let Some(controller) = root["SPBluetoothDataType"].get(0) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, connected) in [("device_connected", true), ("device_not_connected", false)] {
        // Each entry is a one-key object: { "AirPods": { …fields… } }.
        for entry in controller[key].as_array().into_iter().flatten() {
            for (name, d) in entry.as_object().into_iter().flatten() {
                let batteries = [
                    ("Left", "device_batteryLevelLeft"),
                    ("Right", "device_batteryLevelRight"),
                    ("Case", "device_batteryLevelCase"),
                    ("", "device_batteryLevelMain"),
                ]
                .into_iter()
                .filter_map(|(label, k)| {
                    Some(DeviceBattery {
                        label: label.into(),
                        percent: percent(&d[k])?,
                    })
                })
                .collect();
                out.push(
                    BtDevice {
                        name: name.clone(),
                        address: normalize_address(
                            d["device_address"].as_str().unwrap_or_default(),
                        ),
                        kind: kind_of(d["device_minorType"].as_str().unwrap_or_default(), name)
                            .into(),
                        connected,
                        batteries,
                        level: None,
                        case: None,
                        single_bud: None,
                    }
                    .settle(),
                );
            }
        }
    }
    out
}

/// One accessory `ioreg` knows a battery for.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Hid {
    name: String,
    address: String,
    percent: u8,
}

/// `ioreg -r -k BatteryPercent -l` is a tree of `+-o Name <class …>` blocks of
/// `"Key" = value` lines, in no promised order, so a block is collected whole
/// and judged when the next one starts.
fn parse_ioreg(text: &str) -> Vec<Hid> {
    fn flush(block: &mut Option<(Hid, bool)>, out: &mut Vec<Hid>) {
        if let Some((hid, has_battery)) = block.take() {
            if has_battery && !hid.name.is_empty() {
                out.push(hid);
            }
        }
    }
    let mut out = Vec::new();
    let mut block: Option<(Hid, bool)> = None;
    for line in text.lines() {
        if line.contains("+-o ") {
            flush(&mut block, &mut out);
            block = Some((Hid::default(), false));
            continue;
        }
        let Some((hid, has_battery)) = block.as_mut() else {
            continue;
        };
        let Some((key, value)) = line.trim_start_matches([' ', '|']).split_once(" = ") else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        match key.trim_matches('"') {
            "Product" => hid.name = value.to_string(),
            "DeviceAddress" => hid.address = normalize_address(value),
            "BatteryPercent" => {
                if let Ok(p) = value.parse::<u8>() {
                    hid.percent = p.min(100);
                    *has_battery = true;
                }
            }
            _ => {}
        }
    }
    flush(&mut block, &mut out);
    out
}

/// Adds the HID batteries: onto the profiler's entry for the same address (or,
/// failing an address, the same name) when it has none, else as a new row —
/// the Magic Mouse is often missing from the profiler's connected list.
fn merge_hid(devices: &mut Vec<BtDevice>, hids: Vec<Hid>) {
    for hid in hids {
        let battery = DeviceBattery {
            label: String::new(),
            percent: hid.percent,
        };
        let existing = devices
            .iter_mut()
            .find(|d| (!hid.address.is_empty() && d.address == hid.address) || d.name == hid.name);
        match existing {
            Some(d) => {
                d.connected = true;
                if d.batteries.is_empty() {
                    d.batteries.push(battery);
                    *d = d.clone().settle();
                }
            }
            None => devices.push(
                BtDevice {
                    kind: kind_of("", &hid.name).into(),
                    name: hid.name,
                    address: hid.address,
                    connected: true,
                    batteries: vec![battery],
                    level: None,
                    case: None,
                    single_bud: None,
                }
                .settle(),
            ),
        }
    }
}

/// Connects a paired device, over IOBluetooth through JXA's Objective-C bridge:
/// public API, no helper binary. `address` is only ever a value this module
/// produced, but it travels as the script's argument all the same.
pub fn connect(address: &str) -> Result<()> {
    // Validated rather than trusted: the argument goes to a framework call.
    if normalize_address(address).split(':').count() != 6 {
        bail!("{address:?} is not a Bluetooth address");
    }
    let out = run_jxa(
        "Bluetooth",
        // JXA bridges a BOOL getter as a property, so `isPaired` is read, not
        // called; for an address macOS has never seen `d` is nil and it is undefined.
        r#"ObjC.import("IOBluetooth");
        const d = $.IOBluetoothDevice.deviceWithAddressString(ARG.address.replace(/:/g, "-"));
        if (d.isPaired !== true) return "unknown";
        if (d.isConnected === true) return "ok";
        return d.openConnection() === 0 ? "ok" : "failed";"#,
        &json!({ "address": normalize_address(address) }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    match out.as_str() {
        "ok" => Ok(()),
        "unknown" => bail!("that device is not paired with this Mac"),
        _ => bail!("the device did not connect — is it on and in range?"),
    }
}

// ---- connect / disconnect edges ---------------------------------------------

/// A Bluetooth device connected or disconnected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtEvent {
    /// `aa:bb:cc:dd:ee:ff`, lowercase, like [`BtDevice::address`].
    pub address: String,
    pub name: String,
    /// Its Bluetooth class says audio (headphones, headsets, speakers): the
    /// only devices worth a whisper. A keyboard reconnects after every wake.
    pub audio: bool,
    pub connected: bool,
}

/// IOBluetooth can announce the same edge twice (a device with several
/// profiles, or one this process armed twice); inside this window the second
/// is the first again.
const DEBOUNCE: Duration = Duration::from_secs(4);
/// Battery fields reach the profiler a second or two after a connect.
const SETTLE: Duration = Duration::from_millis(1500);
/// At or under this a battery is low; it must climb back to `REARM` (charging)
/// before it can be low again: once per discharge, not once per reading.
const LOW: u8 = 20;
const REARM: u8 = 25;

/// The part of a device that decides when to charge it: the emptier bud, or
/// the single battery. Never the case: it is charged by putting the buds in
/// it, and a low case says nothing about what is in the ears.
fn lowest(batteries: &[DeviceBattery]) -> Option<&DeviceBattery> {
    batteries
        .iter()
        .filter(|b| b.label != "Case")
        .min_by_key(|b| b.percent)
}

#[derive(Debug, Default)]
struct Tracker {
    edges: HashMap<(String, bool), Instant>,
    low: HashSet<String>,
}

impl Tracker {
    /// False for an edge already seen within [`DEBOUNCE`].
    fn accept(&mut self, address: &str, connected: bool, now: Instant) -> bool {
        let key = (address.to_string(), connected);
        if self
            .edges
            .get(&key)
            .is_some_and(|t| now.duration_since(*t) < DEBOUNCE)
        {
            return false;
        }
        self.edges.insert(key, now);
        true
    }

    /// The battery that has just gone low, once per discharge.
    fn low_fires(&mut self, d: &BtDevice) -> Option<DeviceBattery> {
        // A disconnected device's reading is the last one it sent, not news.
        if !d.connected {
            return None;
        }
        let part = lowest(&d.batteries)?;
        let key = if d.address.is_empty() {
            &d.name
        } else {
            &d.address
        };
        if part.percent >= REARM {
            self.low.remove(key);
        } else if part.percent <= LOW && self.low.insert(key.clone()) {
            return Some(part.clone());
        }
        None
    }
}

static TRACKER: Mutex<Option<Tracker>> = Mutex::new(None);

fn with_tracker<R>(f: impl FnOnce(&mut Tracker) -> R) -> R {
    f(TRACKER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_default())
}

fn alert(id: String, icon: &str, title: String, tier: IslandTier) -> Island {
    Island {
        id,
        kind: IslandKind::Alert,
        tier: Some(tier),
        priority: IslandPriority::Normal,
        source: IslandSource::Connectivity,
        icon: Some(icon.into()),
        title,
        detail: Some("Bluetooth".into()),
        ..Island::default()
    }
}

/// The whisper for headphones coming on: the buds' level (the mean of left
/// and right, as the Devices card headlines), or the bare fact while no
/// battery has been reported yet.
fn connected_island(address: &str, name: &str, percent: Option<u8>) -> Island {
    Island {
        short: Some(percent.map_or_else(|| "Connected".into(), |p| format!("{p}%"))),
        ..alert(
            format!("bt:{address}"),
            "headphones",
            name.into(),
            IslandTier::Whisper,
        )
    }
}

/// Same id as the connect whisper, so it replaces one still on screen.
fn disconnected_island(address: &str, name: &str) -> Island {
    Island {
        short: Some("Off".into()),
        ..alert(
            format!("bt:{address}"),
            "headphones",
            name.into(),
            IslandTier::Whisper,
        )
    }
}

fn low_island(d: &BtDevice, part: &DeviceBattery) -> Island {
    let what = if part.label.is_empty() {
        "battery".to_string()
    } else {
        format!("{} battery", part.label.to_lowercase())
    };
    let icon = match d.kind.as_str() {
        "headphones" | "mouse" | "keyboard" | "trackpad" => d.kind.as_str(),
        _ => "battery",
    };
    alert(
        format!("bt:low:{}", d.address),
        icon,
        format!("{} {what} is at {}%", d.name, part.percent),
        IslandTier::Peek,
    )
}

/// A peek for each connected device whose battery has just gone low. Run it
/// on any fresh list: the one a connect reads, the one the Devices view asks
/// for, one at launch. No timer exists only to call this.
pub fn check_batteries(devices: &[BtDevice]) -> Vec<Island> {
    with_tracker(|t| {
        devices
            .iter()
            .filter_map(|d| t.low_fires(d).map(|part| low_island(d, &part)))
            .collect()
    })
}

/// What an edge is worth showing. Blocking on a connect (it waits for the
/// batteries to appear, then reads the profiler once): call it off the run loop.
pub fn react(event: BtEvent) -> Vec<Island> {
    let fresh = with_tracker(|t| t.accept(&event.address, event.connected, Instant::now()));
    if !fresh {
        return Vec::new();
    }
    if !event.connected {
        return if event.audio {
            vec![disconnected_island(&event.address, &event.name)]
        } else {
            Vec::new()
        };
    }
    std::thread::sleep(SETTLE);
    let devices = list().unwrap_or_default();
    let mut out = Vec::new();
    if event.audio {
        let me = devices.iter().find(|d| d.address == event.address);
        out.push(connected_island(
            &event.address,
            me.map_or(event.name.as_str(), |d| d.name.as_str()),
            me.and_then(|d| d.level),
        ));
    }
    out.extend(check_batteries(&devices));
    out
}

/// Starts delivering [`BtEvent`]s to `sink` for the life of the process. Call
/// it on the main thread, and keep the main run loop running (an app's always
/// is; a command-line example pumps it). A second call does nothing. `sink`
/// runs on the main run loop, so it must return quickly.
#[cfg(target_os = "macos")]
pub fn watch(sink: impl Fn(BtEvent) + Send + Sync + 'static) {
    iobt::start(Box::new(sink));
}

/// No Bluetooth notifications off macOS yet: no edges, and so no whispers.
#[cfg(not(target_os = "macos"))]
pub fn watch(_sink: impl Fn(BtEvent) + Send + Sync + 'static) {}

/// IOBluetooth: `+registerForConnectNotifications:selector:` for every device
/// that connects from now on, and `-registerForDisconnectNotification:selector:`
/// per connected device, because macOS offers no global disconnect.
///
/// The callbacks arrive on the main run loop, whatever thread registered: a
/// probe that registered from a thread of its own, running its own `CFRunLoop`,
/// found that loop had nothing scheduled and would never have heard an edge. So
/// there is no thread here. [`watch`] registers on its caller, and the app's
/// main thread (where Tauri's `setup` runs, and whose run loop is the app's)
/// delivers. Callbacks only forward an event; the profiler read and the
/// takeover happen elsewhere.
#[cfg(target_os = "macos")]
mod iobt {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock, PoisonError};

    use objc2::rc::{autoreleasepool, Retained};
    use objc2::runtime::{AnyClass, AnyObject, NSObject};
    use objc2::{define_class, msg_send, sel, AnyThread};
    use objc2_foundation::{NSArray, NSString};

    use super::{normalize_address, BtEvent};

    // Loads the framework, so `IOBluetoothDevice` is a class the runtime knows.
    #[link(name = "IOBluetooth", kind = "framework")]
    unsafe extern "C" {}

    type Sink = Box<dyn Fn(BtEvent) + Send + Sync>;

    // ponytail: one watcher per process, so the Objective-C callbacks reach
    // their sink through a static. Per-observer ivars are the upgrade path.
    static SINK: OnceLock<Sink> = OnceLock::new();
    /// Addresses with a disconnect registration outstanding: connecting twice
    /// must not stack a second one on the same device.
    static ARMED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

    fn armed<R>(f: impl FnOnce(&mut HashSet<String>) -> R) -> R {
        f(ARMED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert_default())
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements and this class
        // implements no Drop.
        #[unsafe(super(NSObject))]
        #[name = "AloudBluetoothObserver"]
        struct Observer;

        impl Observer {
            #[unsafe(method(deviceConnected:device:))]
            fn device_connected(&self, _note: &AnyObject, device: &AnyObject) {
                autoreleasepool(|_| {
                    let event = describe(device, true);
                    self.arm(device, &event.address);
                    emit(event);
                });
            }

            #[unsafe(method(deviceDisconnected:fromDevice:))]
            fn device_disconnected(&self, note: &AnyObject, device: &AnyObject) {
                autoreleasepool(|_| {
                    let event = describe(device, false);
                    armed(|a| a.remove(&event.address));
                    // One registration is one disconnect; the next connect arms anew.
                    // SAFETY: `note` is the IOBluetoothUserNotification we were given.
                    let () = unsafe { msg_send![note, unregister] };
                    emit(event);
                });
            }
        }
    );

    impl Observer {
        fn arm(&self, device: &AnyObject, address: &str) {
            if !armed(|a| a.insert(address.to_string())) {
                return;
            }
            // SAFETY: instance method of IOBluetoothDevice; the observer outlives
            // the registration (its thread never ends) and the selector matches
            // the method defined above.
            let _note: Option<Retained<AnyObject>> = unsafe {
                msg_send![
                    device,
                    registerForDisconnectNotification: self,
                    selector: sel!(deviceDisconnected:fromDevice:)
                ]
            };
        }
    }

    fn describe(device: &AnyObject, connected: bool) -> BtEvent {
        // SAFETY: plain property getters of IOBluetoothDevice.
        let (address, name, major): (Option<Retained<NSString>>, Option<Retained<NSString>>, u32) = unsafe {
            (
                msg_send![device, addressString],
                msg_send![device, name],
                msg_send![device, deviceClassMajor],
            )
        };
        BtEvent {
            address: normalize_address(&address.map(|a| a.to_string()).unwrap_or_default()),
            name: name.map(|n| n.to_string()).unwrap_or_default(),
            // kBluetoothDeviceClassMajorAudio.
            audio: major == 4,
            connected,
        }
    }

    fn emit(event: BtEvent) {
        if let Some(sink) = SINK.get() {
            sink(event);
        }
    }

    pub fn start(sink: Sink) {
        if SINK.set(sink).is_err() {
            return;
        }
        // The observer is never released: its registrations live as long as the process.
        std::mem::forget(autoreleasepool(|_| register()));
    }

    fn register() -> Retained<Observer> {
        let observer: Retained<Observer> =
            // SAFETY: NSObject's designated initialiser.
            unsafe { msg_send![super(Observer::alloc().set_ivars(())), init] };
        let Some(class) = AnyClass::get(c"IOBluetoothDevice") else {
            tracing::warn!("IOBluetooth is not available");
            return observer;
        };
        // SAFETY: class method of IOBluetoothDevice with this selector shape.
        let note: Option<Retained<AnyObject>> = unsafe {
            msg_send![
                class,
                registerForConnectNotifications: &*observer,
                selector: sel!(deviceConnected:device:)
            ]
        };
        // Held for the process: it is what the registration lives in. None means
        // IOBluetooth refused (Bluetooth off or denied) and no edge will come.
        tracing::debug!(
            registered = note.is_some(),
            "IOBluetooth connect notifications"
        );
        std::mem::forget(note);
        // Notifications cover devices that connect from now on; the ones already
        // connected need their disconnect watched too.
        // SAFETY: class method returning an NSArray of IOBluetoothDevice, or nil.
        let paired: Option<Retained<NSArray<AnyObject>>> =
            unsafe { msg_send![class, pairedDevices] };
        for device in paired.iter().flat_map(|a| a.iter()) {
            // SAFETY: property getter.
            let connected: bool = unsafe { msg_send![&*device, isConnected] };
            if connected {
                let address = describe(&device, true).address;
                tracing::debug!(%address, "watching an already connected device");
                observer.arm(&device, &address);
            }
        }
        observer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILER: &str = r#"{"SPBluetoothDataType":[{"controller_properties":{},
      "device_connected":[
        {"Yaksh’s AirPods Pro":{"device_address":"AA:BB:CC:DD:EE:01","device_batteryLevelCase":"63%",
          "device_batteryLevelLeft":"100%","device_batteryLevelRight":"98%","device_minorType":"Headphones"}},
        {"Magic Keyboard":{"device_address":"AA:BB:CC:DD:EE:02","device_batteryLevelMain":"41%","device_minorType":"Keyboard"}}],
      "device_not_connected":[
        {"RORSOU C6":{"device_address":"0D:16:4A:D5:46:CF","device_minorType":"Headset"}}]}]}"#;

    #[test]
    fn the_profiler_gives_each_battery_its_label() {
        let devices = parse_profiler(PROFILER);
        assert_eq!(devices.len(), 3);
        let pods = &devices[0];
        assert!(pods.connected);
        assert_eq!(pods.kind, "headphones");
        assert_eq!(
            pods.batteries,
            vec![
                DeviceBattery {
                    label: "Left".into(),
                    percent: 100
                },
                DeviceBattery {
                    label: "Right".into(),
                    percent: 98
                },
                DeviceBattery {
                    label: "Case".into(),
                    percent: 63
                },
            ]
        );
        assert_eq!(
            devices[1].batteries,
            vec![DeviceBattery {
                label: String::new(),
                percent: 41
            }]
        );
        assert!(!devices[2].connected && devices[2].batteries.is_empty());
        assert_eq!(devices[2].address, "0d:16:4a:d5:46:cf");
    }

    /// A trimmed real `system_profiler SPBluetoothDataType -json`: other keys
    /// around the batteries, one pair of buds with one bud out, a headset
    /// with a single battery, a mouse the profiler also lists as not connected.
    const REAL: &str = r#"{"SPBluetoothDataType":[{"controller_properties":{"controller_state":"attrib_on"},
      "device_connected":[
        {"Yaksh’s AirPods Pro":{"device_address":"AA:BB:CC:DD:EE:01","device_batteryLevelCase":"63%",
          "device_batteryLevelLeft":"100%","device_batteryLevelRight":"98%","device_firmwareVersion":"7E93",
          "device_minorType":"Headphones","device_productID":"0x2014","device_vendorID":"0x004C"}},
        {"Beats Fit Pro":{"device_address":"AA:BB:CC:DD:EE:04","device_batteryLevelLeft":"41%",
          "device_batteryLevelCase":"90%","device_minorType":"Headphones"}},
        {"Sony WH-1000XM5":{"device_address":"AA:BB:CC:DD:EE:05","device_batteryLevelMain":"77%","device_minorType":"Headset"}}],
      "device_not_connected":[
        {"Magic Mouse":{"device_address":"AA:BB:CC:DD:EE:06","device_minorType":"Mouse"}}]}]}"#;

    /// Guards: AirPods headlined 63%, the case, and the buds were never
    /// averaged. One device is one entry however many parts it reports.
    #[test]
    fn buds_average_the_case_is_apart_and_one_device_is_one_entry() {
        let devices = parse_profiler(REAL);
        assert_eq!(
            devices.len(),
            4,
            "one entry per device, not per battery part"
        );
        let pods = &devices[0];
        assert_eq!(
            (pods.level, pods.case, pods.single_bud.clone()),
            (Some(99), Some(63), None)
        );
        // One bud out: that bud alone, said so, and the case still apart.
        let beats = &devices[1];
        assert_eq!(
            (beats.level, beats.case, beats.single_bud.as_deref()),
            (Some(41), Some(90), Some("Left"))
        );
        let headset = &devices[2];
        assert_eq!(
            (headset.level, headset.case, headset.single_bud.clone()),
            (Some(77), None, None)
        );
        let off = &devices[3];
        assert_eq!((off.connected, off.level, off.case), (false, None, None));
    }

    #[test]
    fn the_mean_rounds_half_up_and_a_case_alone_has_no_headline() {
        assert_eq!(pods(99, 100, 10).level, Some(100));
        assert_eq!(pods(0, 1, 10).level, Some(1));
        let case_only = BtDevice {
            batteries: vec![DeviceBattery {
                label: "Case".into(),
                percent: 55,
            }],
            ..pods(0, 0, 0)
        }
        .settle();
        assert_eq!((case_only.level, case_only.case), (None, Some(55)));
    }

    /// Guards: a nearly empty case raised "AirPods case battery is at 12%" while
    /// the buds were full; the case is not what runs out in the ears.
    #[test]
    fn a_low_case_alone_is_not_low() {
        let mut t = Tracker::default();
        assert!(t.low_fires(&pods(90, 95, 12)).is_none());
        assert!(
            t.low_fires(&pods(90, 15, 12)).is_some(),
            "an emptied bud is"
        );
    }

    #[test]
    fn nothing_paired_or_garbage_is_an_empty_list() {
        assert!(parse_profiler("{}").is_empty());
        assert!(parse_profiler("not json").is_empty());
        let bare = r#"{"SPBluetoothDataType":[{"controller_properties":{}}]}"#;
        assert!(parse_profiler(bare).is_empty());
    }

    const IOREG: &str = r#"+-o Magic Mouse  <class AppleDeviceManagementHIDEventService, id 0x1, registered>
    |   {
    |     "BatteryPercent" = 87
    |     "Product" = "Magic Mouse"
    |     "DeviceAddress" = "aa-bb-cc-dd-ee-03"
    |   }
    |
+-o Magic Trackpad  <class AppleDeviceManagementHIDEventService, id 0x2, registered>
    |   {
    |     "Product" = "Magic Trackpad"
    |     "BatteryPercent" = 100
    |   }
+-o Something Without A Battery  <class IOService, id 0x3>
    |   {
    |     "Product" = "Nope"
    |   }
"#;

    /// Guards: ioreg lists a block's keys in no fixed order, and reading
    /// the battery before the product name dropped the device.
    #[test]
    fn ioreg_blocks_are_read_whole_whatever_the_key_order() {
        let hids = parse_ioreg(IOREG);
        assert_eq!(hids.len(), 2);
        assert_eq!(
            hids[0],
            Hid {
                name: "Magic Mouse".into(),
                address: "aa:bb:cc:dd:ee:03".into(),
                percent: 87
            }
        );
        assert_eq!(hids[1].name, "Magic Trackpad");
        assert_eq!(hids[1].percent, 100);
    }

    #[test]
    fn hid_batteries_fill_gaps_and_add_missing_devices() {
        let mut devices = parse_profiler(PROFILER);
        let hids = vec![
            // Same address as a profiler row that reported no battery of its own.
            Hid {
                name: "Magic Keyboard".into(),
                address: "aa:bb:cc:dd:ee:02".into(),
                percent: 10,
            },
            Hid {
                name: "Magic Mouse".into(),
                address: "aa:bb:cc:dd:ee:03".into(),
                percent: 87,
            },
        ];
        merge_hid(&mut devices, hids);

        assert_eq!(devices.len(), 4);
        let keyboard = devices.iter().find(|d| d.name == "Magic Keyboard").unwrap();
        assert_eq!(
            keyboard.batteries[0].percent, 41,
            "the profiler's own figure wins"
        );
        let mouse = devices.iter().find(|d| d.name == "Magic Mouse").unwrap();
        assert_eq!((mouse.connected, mouse.kind.as_str()), (true, "mouse"));
    }

    /// Guards: a battery field that is not a plain percentage ("N/A", "120%")
    /// must drop the figure, not show a bar of nonsense or hide the device.
    #[test]
    fn a_bad_battery_figure_is_dropped_but_the_device_stays() {
        let json = r#"{"SPBluetoothDataType":[{"device_connected":[
          {"Buds":{"device_address":"AA-BB-CC-DD-EE-09","device_batteryLevelLeft":"N/A",
            "device_batteryLevelRight":"120%","device_batteryLevelCase":" 55% "}}]}]}"#;
        let devices = parse_profiler(json);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].address, "aa:bb:cc:dd:ee:09");
        assert_eq!(devices[0].kind, "headphones");
        assert_eq!(devices[0].batteries.len(), 1);
        assert_eq!(devices[0].batteries[0].percent, 55);
    }

    #[test]
    fn connect_refuses_what_is_not_an_address() {
        assert!(connect("--help").is_err());
        assert!(connect("").is_err());
    }

    fn pods(left: u8, right: u8, case: u8) -> BtDevice {
        let part = |label: &str, percent| DeviceBattery {
            label: label.into(),
            percent,
        };
        BtDevice {
            name: "AirPods Pro".into(),
            address: "aa:bb:cc:dd:ee:01".into(),
            kind: "headphones".into(),
            connected: true,
            batteries: vec![part("Left", left), part("Right", right), part("Case", case)],
            level: None,
            case: None,
            single_bud: None,
        }
        .settle()
    }

    /// Guards: the whisper (and the card) headlined 63%, the case, for
    /// AirPods whose buds were at 100% and 98%.
    #[test]
    fn the_buds_speak_for_the_device_never_the_case() {
        let d = pods(92, 88, 64);
        assert_eq!(d.level, Some(90));
        assert_eq!(lowest(&d.batteries).map(|b| b.percent), Some(88));
        assert_eq!(lowest(&[]), None);
        let island = connected_island(&d.address, &d.name, d.level);
        assert_eq!(island.short.as_deref(), Some("90%"));
        assert_eq!(island.tier, Some(IslandTier::Whisper));
        assert_eq!(island.source, IslandSource::Connectivity);
        let bare = connected_island(&d.address, &d.name, None);
        assert_eq!(bare.short.as_deref(), Some("Connected"));
    }

    /// Guards: a connect that IOBluetooth announces twice in a burst drew two
    /// whispers; a reconnect after the window is a real one.
    #[test]
    fn a_repeated_edge_inside_the_window_is_one_edge() {
        let mut t = Tracker::default();
        let t0 = Instant::now();
        assert!(t.accept("a", true, t0));
        assert!(!t.accept("a", true, t0 + Duration::from_secs(1)));
        assert!(
            t.accept("a", false, t0 + Duration::from_secs(1)),
            "a disconnect is its own edge"
        );
        assert!(t.accept("b", true, t0 + Duration::from_secs(1)));
        assert!(t.accept("a", true, t0 + DEBOUNCE));
    }

    /// Guards: a battery sitting at 19% re-announced on every connect and every
    /// Devices poll. Once per discharge: it re-arms only after charging past 25%.
    #[test]
    fn low_battery_is_announced_once_per_discharge() {
        let mut t = Tracker::default();
        let part = t.low_fires(&pods(90, 19, 50)).expect("first low reading");
        assert_eq!((part.label.as_str(), part.percent), ("Right", 19));
        assert!(t.low_fires(&pods(90, 19, 50)).is_none(), "same discharge");
        assert!(
            t.low_fires(&pods(90, 12, 50)).is_none(),
            "still the same one"
        );
        assert!(
            t.low_fires(&pods(90, 22, 50)).is_none(),
            "not charged enough to re-arm"
        );
        assert!(t.low_fires(&pods(90, 20, 50)).is_none());
        assert!(t.low_fires(&pods(90, 80, 50)).is_none(), "charged");
        assert!(t.low_fires(&pods(90, 20, 50)).is_some(), "a new discharge");
    }

    #[test]
    fn a_disconnected_device_or_a_healthy_one_is_never_low() {
        let mut t = Tracker::default();
        let mut gone = pods(5, 5, 5);
        gone.connected = false;
        assert!(t.low_fires(&gone).is_none());
        assert!(t.low_fires(&pods(21, 40, 90)).is_none(), "21% is not low");
        assert!(t.low_fires(&pods(50, 50, 100)).is_none());
        let island = low_island(
            &pods(90, 15, 50),
            &DeviceBattery {
                label: "Right".into(),
                percent: 15,
            },
        );
        assert_eq!(island.tier, Some(IslandTier::Peek));
        assert_eq!(island.title, "AirPods Pro right battery is at 15%");
        assert_eq!(island.icon.as_deref(), Some("headphones"));
    }

    #[test]
    fn a_disconnect_replaces_the_connect_whisper() {
        let off = disconnected_island("aa:bb", "AirPods Pro");
        assert_eq!(off.id, connected_island("aa:bb", "AirPods Pro", Some(1)).id);
        assert_eq!(off.short.as_deref(), Some("Off"));
    }

    /// Guards: a keyboard reconnecting after every wake is not news, and a
    /// disconnect of one must not even look at the profiler.
    #[test]
    fn only_audio_devices_whisper_on_disconnect() {
        let edge = |audio| BtEvent {
            address: "aa:bb:cc:dd:ee:77".into(),
            name: "Thing".into(),
            audio,
            connected: false,
        };
        assert!(react(edge(false)).is_empty());
        let shown = react(BtEvent {
            address: "aa:bb:cc:dd:ee:78".into(),
            ..edge(true)
        });
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].short.as_deref(), Some("Off"));
    }
}
