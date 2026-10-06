//! Drives: which entries in `/Volumes` are external and ejectable, a watcher
//! for them coming and going, and the eject itself — all on `diskutil`.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// How often `/Volumes` is looked at; a plugged-in drive shows up within this.
const POLL: Duration = Duration::from_millis(1500);

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::watch::{self, Handle};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[serde(rename = "ShelfDrive")]
pub struct Drive {
    pub name: String,
    /// `/Volumes/<name>` — what `diskutil eject` is given.
    pub mount: String,
    /// How it is attached, in diskutil's words: "USB", "Disk Image",
    /// "Secure Digital".
    pub bus: String,
    #[specta(type = specta_typescript::Number)]
    pub size: u64,
    #[specta(type = specta_typescript::Number)]
    pub free: u64,
}

/// The slice of `diskutil info -plist` that decides whether a volume is one
/// to offer an eject for.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Info {
    volume_name: Option<String>,
    mount_point: Option<String>,
    #[serde(default)]
    internal: bool,
    #[serde(default)]
    ejectable: bool,
    #[serde(default)]
    removable_media: bool,
    bus_protocol: Option<String>,
    #[serde(default)]
    total_size: u64,
    #[serde(default)]
    free_space: u64,
}

/// A drive from `diskutil info`'s JSON (after `plutil -convert json`), or
/// None for what is not offered: the startup disk, internal volumes, and
/// anything macOS will not let you eject.
///
/// Ejectable and *either* not internal *or* removable media: a MacBook's SD
/// slot reports `Internal` but is removable, and is still the thing someone
/// wants to eject. A disk image (a mounted DMG) qualifies too — ejecting one
/// after installing an app is the commonest eject there is.
pub fn parse_info(json: &str) -> Option<Drive> {
    let info: Info = serde_json::from_str(json).ok()?;
    if !info.ejectable || (info.internal && !info.removable_media) {
        return None;
    }
    let mount = info.mount_point.filter(|m| !m.is_empty())?;
    let name = info
        .volume_name
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            Path::new(&mount)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
    Some(Drive {
        name,
        mount,
        bus: info.bus_protocol.unwrap_or_default(),
        size: info.total_size,
        free: info.free_space,
    })
}

fn info(mount: &Path) -> Option<Drive> {
    // `plutil` turns diskutil's plist into JSON on the way through; there is
    // no plist parser in the tree and none is worth adding for one call.
    let plist = std::process::Command::new("diskutil")
        .args(["info", "-plist"])
        .arg(mount)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let mut convert = std::process::Command::new("plutil")
        .args(["-convert", "json", "-o", "-", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    {
        use std::io::Write;
        convert.stdin.take()?.write_all(&plist.stdout).ok()?;
    }
    let out = convert.wait_with_output().ok()?;
    parse_info(&String::from_utf8_lossy(&out.stdout))
}

/// Every external drive mounted under `/Volumes`, by name.
pub fn list() -> Vec<Drive> {
    list_in(Path::new("/Volumes"))
}

fn list_in(volumes: &Path) -> Vec<Drive> {
    let Ok(entries) = std::fs::read_dir(volumes) else {
        return Vec::new();
    };
    let mounts: Vec<PathBuf> = entries
        .flatten()
        // `/Volumes/Macintosh HD` is a symlink to `/`: not a drive to offer,
        // and asking diskutil about it is a wasted spawn.
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    // Two process spawns per volume, and a Mac with a dozen mounted DMGs
    // pays for it in seconds when they run one after another.
    let mut drives: Vec<Drive> = std::thread::scope(|scope| {
        let jobs: Vec<_> = mounts.iter().map(|m| scope.spawn(|| info(m))).collect();
        jobs.into_iter()
            .filter_map(|j| j.join().ok().flatten())
            .collect()
    });
    drives.sort_by_key(|d| d.name.to_lowercase());
    drives
}

/// What a re-listing changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Added(Drive),
    Removed(Drive),
}

/// Drives that appeared or went between two listings, keyed by mount point.
pub fn diff(old: &[Drive], new: &[Drive]) -> Vec<Change> {
    let gone = old
        .iter()
        .filter(|o| !new.iter().any(|n| n.mount == o.mount))
        .cloned()
        .map(Change::Removed);
    let came = new
        .iter()
        .filter(|n| !old.iter().any(|o| o.mount == n.mount))
        .cloned()
        .map(Change::Added);
    gone.chain(came).collect()
}

/// Reports each drive as it is mounted or unmounted. The first listing is
/// taken now, so drives already plugged in are not announced as new.
pub fn watch(on: impl Fn(Change) + Send + 'static) -> Result<Handle> {
    let mut known = list();
    Ok(watch::names_changed(
        Path::new("/Volumes"),
        POLL,
        move || {
            let now = list();
            for change in diff(&known, &now) {
                on(change);
            }
            known = now;
        },
    ))
}

/// Unmounts and ejects. A busy drive is an `Err` with diskutil's own words
/// ("dissented by PID 612 (Finder)"), which names what is holding it.
pub fn eject(mount: &Path) -> Result<()> {
    // Only something `list` would offer: a path from the webview must not be
    // able to point `diskutil eject` at a volume the user was never shown.
    if mount.parent() != Some(Path::new("/Volumes")) {
        anyhow::bail!("{} is not a drive", mount.display());
    }
    crate::run("diskutil", &["eject".as_ref(), mount.as_os_str()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DMG: &str = include_str!("../tests/diskutil-dmg.json");
    const STARTUP: &str = include_str!("../tests/diskutil-internal.json");

    fn drive(mount: &str) -> Drive {
        Drive {
            name: mount.into(),
            mount: format!("/Volumes/{mount}"),
            bus: "USB".into(),
            size: 1,
            free: 1,
        }
    }

    #[test]
    fn a_mounted_disk_image_is_offered_with_its_sizes() {
        let d = parse_info(DMG).expect("a DMG is ejectable");
        assert_eq!(d.name, "Slack");
        assert_eq!(d.mount, "/Volumes/Slack");
        assert_eq!(d.bus, "Disk Image");
        assert_eq!(d.size, 906_452_992);
    }

    #[test]
    fn the_startup_disk_is_never_offered() {
        assert_eq!(parse_info(STARTUP), None);
    }

    #[test]
    fn an_internal_sd_slot_counts_but_an_internal_fixed_disk_does_not() {
        let sd = r#"{"VolumeName":"CARD","MountPoint":"/Volumes/CARD","Internal":true,
            "Ejectable":true,"RemovableMedia":true,"BusProtocol":"Secure Digital"}"#;
        assert!(parse_info(sd).is_some());
        let fixed = r#"{"VolumeName":"Data","MountPoint":"/Volumes/Data","Internal":true,
            "Ejectable":true,"RemovableMedia":false}"#;
        assert_eq!(parse_info(fixed), None);
    }

    #[test]
    fn a_volume_that_cannot_be_ejected_is_not_offered() {
        let nas = r#"{"VolumeName":"NAS","MountPoint":"/Volumes/NAS","Internal":false,"Ejectable":false}"#;
        assert_eq!(parse_info(nas), None);
        assert_eq!(parse_info("not json"), None);
    }

    #[test]
    fn a_nameless_volume_takes_its_mount_folders_name() {
        let j = r#"{"MountPoint":"/Volumes/UNTITLED","Internal":false,"Ejectable":true}"#;
        assert_eq!(parse_info(j).unwrap().name, "UNTITLED");
    }

    #[test]
    fn diff_names_what_came_and_went_by_mount_point() {
        let old = [drive("A"), drive("B")];
        let new = [drive("B"), drive("C")];
        assert_eq!(
            diff(&old, &new),
            vec![Change::Removed(drive("A")), Change::Added(drive("C"))]
        );
        assert!(diff(&new, &new).is_empty());
    }

    #[test]
    fn eject_refuses_anything_outside_volumes() {
        assert!(eject(Path::new("/")).is_err());
        assert!(eject(Path::new("/Volumes/a/b")).is_err());
        assert!(eject(Path::new("/Users/me")).is_err());
    }

    /// Against the real machine: the startup disk's symlink in `/Volumes`
    /// must never show up, whatever else is mounted.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_real_listing_has_no_startup_disk() {
        assert!(list().iter().all(|d| d.mount != "/"));
    }
}
