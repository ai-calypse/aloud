//! Bluetooth gear and its batteries: AirPods (left, right, case), headphones,
//! and the Magic Mouse/Keyboard/Trackpad.
//!
//! Two sources, merged: `system_profiler SPBluetoothDataType -json` knows every
//! paired device and the batteries it reports (AirPods, most headphones);
//! `ioreg` knows the Apple HID accessories' `BatteryPercent`.
//!
//! ponytail: no "AirPods case nearby" popup. That is BLE scanning of Apple's
//! proximity packets, a Swift helper's job, not a subprocess's.

use std::process::Command;

use aloud_native::run_jxa;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
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
    pub batteries: Vec<Battery>,
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
                    Some(Battery {
                        label: label.into(),
                        percent: percent(&d[k])?,
                    })
                })
                .collect();
                out.push(BtDevice {
                    name: name.clone(),
                    address: normalize_address(d["device_address"].as_str().unwrap_or_default()),
                    kind: kind_of(d["device_minorType"].as_str().unwrap_or_default(), name).into(),
                    connected,
                    batteries,
                });
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
        let battery = Battery {
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
                }
            }
            None => devices.push(BtDevice {
                kind: kind_of("", &hid.name).into(),
                name: hid.name,
                address: hid.address,
                connected: true,
                batteries: vec![battery],
            }),
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
                Battery {
                    label: "Left".into(),
                    percent: 100
                },
                Battery {
                    label: "Right".into(),
                    percent: 98
                },
                Battery {
                    label: "Case".into(),
                    percent: 63
                },
            ]
        );
        assert_eq!(
            devices[1].batteries,
            vec![Battery {
                label: String::new(),
                percent: 41
            }]
        );
        assert!(!devices[2].connected && devices[2].batteries.is_empty());
        assert_eq!(devices[2].address, "0d:16:4a:d5:46:cf");
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
}
