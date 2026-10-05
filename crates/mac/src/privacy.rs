//! Which apps are using the microphone, and whether the camera is on — the
//! orange and green dots, for the notch.
//!
//! All public CoreAudio / CoreMediaIO properties, no entitlement and no prompt:
//!
//! - **Mic, macOS 14.2+:** the HAL's process list, each with `IsRunningInput`.
//!   That names the app and lets Aloud leave out its own dictation, which
//!   would otherwise light the indicator on every hotkey press.
//! - **Mic, older:** `DeviceIsRunningSomewhere` on every input device. No
//!   names, and Aloud's own capture counts. ponytail: an older macOS shows
//!   Aloud's dictation as mic use; there is no public per-process answer
//!   before 14.2.
//! - **Camera:** `DeviceIsRunningSomewhere` on every CMIO device. CMIO has no
//!   per-process property, so the camera is on or off, never by name.

use aloud_islands::{Island, IslandAction, IslandKind, IslandPriority, IslandTier};
use serde::{Deserialize, Serialize};

use crate::Step;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyUse {
    pub mic: bool,
    pub camera: bool,
    /// Apps holding the mic, sorted; empty when only the device-level answer
    /// was available.
    pub mic_apps: Vec<String>,
}

/// One reading, from this process's point of view.
pub fn read() -> PrivacyUse {
    #[cfg(target_os = "macos")]
    {
        macos::read()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PrivacyUse::default()
    }
}

/// A helper process is named for its app: `com.google.Chrome.helper.Renderer`
/// is Chrome. Cut at the first `.helper` segment, any case.
pub fn parent_bundle(id: &str) -> &str {
    let lower = id.to_ascii_lowercase();
    match lower.find(".helper") {
        Some(at) => &id[..at],
        None => id,
    }
}

/// Listeners that hold an input stream all day and are not what anyone means
/// by "the mic is on": CoreSpeech spotting "Hey Siri" showed for 16 of 20
/// seconds on a Mac nobody was talking to.
pub fn is_background_listener(id: &str) -> bool {
    id.is_empty() || id.to_ascii_lowercase().contains("corespeech")
}

/// When there is no running app to ask, the last word of the bundle id.
pub fn fallback_name(id: &str) -> String {
    parent_bundle(id)
        .rsplit('.')
        .next()
        .unwrap_or(id)
        .to_string()
}

pub const ID: &str = "mac:privacy";

/// How many quiet polls in a row end the island. A call app opening and
/// closing its stream between two reads must not blink the indicator.
const QUIET_POLLS: u8 = 2;

pub fn island(u: &PrivacyUse) -> Island {
    let title = match (u.camera, u.mic) {
        (true, true) => "Camera and microphone in use",
        (true, false) => "Camera in use",
        _ => "Microphone in use",
    };
    let detail = match u.mic_apps.as_slice() {
        [] => None,
        [one] => Some(one.clone()),
        [first, rest @ ..] => Some(format!("{first} +{}", rest.len())),
    };
    Island {
        id: ID.into(),
        kind: IslandKind::Live,
        priority: IslandPriority::High,
        icon: Some(if u.camera { "camera" } else { "mic" }.into()),
        title: title.into(),
        detail,
        actions: vec![IslandAction::new("Show", "mac.privacy")],
        expires: None,
        dismissible: false,
        ..Island::default()
    }
}

/// The whisper at the first sighting, beside the standing island: the island
/// is the indicator that stays, this is the moment it started.
pub const SEEN_ID: &str = "mac:privacy-seen";

fn sighting(u: &PrivacyUse) -> Island {
    let short = match (u.camera, u.mic) {
        (true, true) => "Camera + mic",
        (true, false) => "Camera",
        _ => "Mic",
    };
    Island {
        id: SEEN_ID.into(),
        kind: IslandKind::Alert,
        priority: IslandPriority::High,
        tier: Some(IslandTier::Whisper),
        icon: Some(if u.camera { "camera" } else { "mic" }.into()),
        title: island(u).title,
        detail: island(u).detail,
        short: Some(short.into()),
        actions: vec![IslandAction::new("Show", "mac.privacy")],
        ..Island::default()
    }
}

/// Raises the island at the first sighting — privacy news is not worth a
/// delay — and takes it down only after [`QUIET_POLLS`] empty reads.
#[derive(Debug, Default)]
pub struct Watch {
    quiet: u8,
    /// An island is up, so the next in-use read is not a first sighting.
    up: bool,
    fresh: bool,
}

impl Watch {
    pub fn update(&mut self, u: &PrivacyUse) -> Option<Step> {
        if u.mic || u.camera {
            self.quiet = 0;
            self.fresh |= !self.up;
            self.up = true;
            return Some(Step::Push(island(u)));
        }
        self.quiet = self.quiet.saturating_add(1);
        if self.quiet == QUIET_POLLS {
            self.up = false;
            return Some(Step::Remove(ID));
        }
        None
    }

    /// Once per start of use, right after the [`Watch::update`] that saw it.
    pub fn take_sighting(&mut self, u: &PrivacyUse) -> Option<Island> {
        std::mem::take(&mut self.fresh).then(|| sighting(u))
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    use std::ptr::null;

    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;

    use super::{fallback_name, parent_bundle, PrivacyUse};

    const fn fourcc(s: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*s)
    }

    /// `AudioObjectPropertyAddress` and `CMIOObjectPropertyAddress` are the
    /// same three words.
    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }

    const SYSTEM_OBJECT: u32 = 1;
    const GLOBAL: u32 = fourcc(b"glob");
    const INPUT: u32 = fourcc(b"inpt");
    const DEVICES: u32 = fourcc(b"dev#");
    const STREAMS: u32 = fourcc(b"stm#");
    /// `kAudioDevicePropertyDeviceIsRunningSomewhere` and
    /// `kCMIODevicePropertyDeviceIsRunningSomewhere` are both 'gone'.
    const RUNNING_SOMEWHERE: u32 = fourcc(b"gone");
    const PROCESS_LIST: u32 = fourcc(b"prs#");
    const PROCESS_PID: u32 = fourcc(b"ppid");
    const PROCESS_BUNDLE: u32 = fourcc(b"pbid");
    const PROCESS_RUNNING_INPUT: u32 = fourcc(b"piri");

    fn at(selector: u32, scope: u32) -> Address {
        Address {
            selector,
            scope,
            element: 0,
        }
    }

    #[link(name = "CoreAudio", kind = "framework")]
    unsafe extern "C" {
        fn AudioObjectGetPropertyDataSize(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
        fn AudioObjectGetPropertyData(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    #[link(name = "CoreMediaIO", kind = "framework")]
    unsafe extern "C" {
        fn CMIOObjectGetPropertyDataSize(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
        fn CMIOObjectGetPropertyData(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: u32,
            used: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    /// The two frameworks differ only in how they spell the data call, so the
    /// reads below are written once against this.
    struct Source {
        size: fn(u32, &Address) -> Option<u32>,
        data: fn(u32, &Address, *mut c_void, u32) -> bool,
    }

    const AUDIO: Source = Source {
        size: |object, a| {
            let mut n = 0;
            // SAFETY: `a` and `n` outlive the call; no qualifier.
            (unsafe { AudioObjectGetPropertyDataSize(object, a, 0, null(), &mut n) } == 0)
                .then_some(n)
        },
        data: |object, a, out, size| {
            let mut n = size;
            // SAFETY: `out` points at `size` writable bytes, as every caller below arranges.
            unsafe { AudioObjectGetPropertyData(object, a, 0, null(), &mut n, out) == 0 }
        },
    };

    const VIDEO: Source = Source {
        size: |object, a| {
            let mut n = 0;
            // SAFETY: as for AUDIO.
            (unsafe { CMIOObjectGetPropertyDataSize(object, a, 0, null(), &mut n) } == 0)
                .then_some(n)
        },
        data: |object, a, out, size| {
            let mut used = 0;
            // SAFETY: as for AUDIO.
            unsafe { CMIOObjectGetPropertyData(object, a, 0, null(), size, &mut used, out) == 0 }
        },
    };

    impl Source {
        /// An array property of 32-bit ids; None when the property is not
        /// there (an OS too old for it), empty when it is and holds nothing.
        fn ids(&self, object: u32, a: &Address) -> Option<Vec<u32>> {
            let bytes = (self.size)(object, a)?;
            let mut out = vec![0u32; bytes as usize / 4];
            if out.is_empty() {
                return Some(out);
            }
            (self.data)(object, a, out.as_mut_ptr().cast(), bytes).then_some(out)
        }

        fn one(&self, object: u32, a: &Address) -> Option<u32> {
            let mut v = 0u32;
            (self.data)(object, a, (&mut v as *mut u32).cast(), 4).then_some(v)
        }

        fn string(&self, object: u32, a: &Address) -> Option<String> {
            let mut s: CFStringRef = null();
            if !(self.data)(object, a, (&mut s as *mut CFStringRef).cast(), 8) || s.is_null() {
                return None;
            }
            // SAFETY: CoreAudio hands back a +1 CFString for the caller to release.
            Some(unsafe { CFString::wrap_under_create_rule(s) }.to_string())
        }

        fn running(&self, device: u32) -> bool {
            self.one(device, &at(RUNNING_SOMEWHERE, GLOBAL)) == Some(1)
        }
    }

    pub fn read() -> PrivacyUse {
        let (mic, mic_apps) = match mic_apps() {
            Some(apps) => (!apps.is_empty(), apps),
            None => (device_level_mic(), Vec::new()),
        };
        PrivacyUse {
            mic,
            camera: camera(),
            mic_apps,
        }
    }

    /// Apps with an input stream running, without Aloud itself; None before
    /// macOS 14.2, where the process list does not exist.
    fn mic_apps() -> Option<Vec<String>> {
        let processes = AUDIO.ids(SYSTEM_OBJECT, &at(PROCESS_LIST, GLOBAL))?;
        let own = std::process::id();
        let mut apps: Vec<String> = processes
            .into_iter()
            .filter(|p| AUDIO.one(*p, &at(PROCESS_RUNNING_INPUT, GLOBAL)) == Some(1))
            .filter(|p| AUDIO.one(*p, &at(PROCESS_PID, GLOBAL)) != Some(own))
            // Bare binaries and daemons have no bundle id (a `cargo run`
            // recorder is invisible here, on purpose).
            .filter_map(|p| AUDIO.string(p, &at(PROCESS_BUNDLE, GLOBAL)))
            .filter(|id| !super::is_background_listener(id))
            .map(|id| app_name(&id))
            .collect();
        apps.sort();
        apps.dedup();
        Some(apps)
    }

    fn app_name(bundle_id: &str) -> String {
        let parent = NSString::from_str(parent_bundle(bundle_id));
        // SAFETY: read-only LaunchServices queries, callable from any thread.
        unsafe {
            NSRunningApplication::runningApplicationsWithBundleIdentifier(&parent)
                .iter()
                .find_map(|app| app.localizedName())
        }
        .map_or_else(|| fallback_name(bundle_id), |name| name.to_string())
    }

    fn device_level_mic() -> bool {
        let Some(devices) = AUDIO.ids(SYSTEM_OBJECT, &at(DEVICES, GLOBAL)) else {
            return false;
        };
        devices.into_iter().any(|d| {
            // Output-only devices report "running" whenever anything plays.
            let has_input = (AUDIO.size)(d, &at(STREAMS, INPUT)).is_some_and(|n| n > 0);
            has_input && AUDIO.running(d)
        })
    }

    fn camera() -> bool {
        VIDEO
            .ids(SYSTEM_OBJECT, &at(DEVICES, GLOBAL))
            .unwrap_or_default()
            .into_iter()
            .any(|d| VIDEO.running(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn using(mic: bool, camera: bool, apps: &[&str]) -> PrivacyUse {
        PrivacyUse {
            mic,
            camera,
            mic_apps: apps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn a_helper_process_is_named_for_its_app() {
        assert_eq!(
            parent_bundle("com.google.Chrome.helper.Renderer"),
            "com.google.Chrome"
        );
        assert_eq!(parent_bundle("us.zoom.xos"), "us.zoom.xos");
        assert_eq!(fallback_name("com.google.Chrome.Helper"), "Chrome");
        assert_eq!(fallback_name("us.zoom.xos"), "xos");
        assert!(is_background_listener("com.apple.corespeechd"));
        assert!(is_background_listener(""));
        assert!(!is_background_listener("com.apple.FaceTime"));
    }

    #[test]
    fn the_island_names_what_is_on_and_who() {
        let i = island(&using(true, false, &["Zoom"]));
        assert_eq!(
            (i.title.as_str(), i.detail.as_deref(), i.icon.as_deref()),
            ("Microphone in use", Some("Zoom"), Some("mic"))
        );
        let i = island(&using(true, true, &["Zoom", "Slack", "Meet"]));
        assert_eq!(i.title, "Camera and microphone in use");
        assert_eq!(i.detail.as_deref(), Some("Zoom +2"));
        assert_eq!(i.icon.as_deref(), Some("camera"));
        assert_eq!(island(&using(false, true, &[])).detail, None);
    }

    /// The failure: an app that closes and reopens its stream between polls
    /// blinking the indicator off and on.
    #[test]
    fn it_shows_at_once_and_leaves_only_after_two_quiet_polls() {
        let mut watch = Watch::default();
        assert!(matches!(
            watch.update(&using(true, false, &[])),
            Some(Step::Push(_))
        ));
        assert_eq!(watch.update(&PrivacyUse::default()), None);
        assert!(
            matches!(watch.update(&using(false, true, &[])), Some(Step::Push(_))),
            "a blip resets the count"
        );
        assert_eq!(watch.update(&PrivacyUse::default()), None);
        assert_eq!(watch.update(&PrivacyUse::default()), Some(Step::Remove(ID)));
        assert_eq!(
            watch.update(&PrivacyUse::default()),
            None,
            "removed once, not every poll"
        );
    }

    /// The failure: a whisper on every poll while a call is on, which is the
    /// standing island's job — and none when a blip came back within two polls.
    #[test]
    fn the_whisper_marks_the_start_of_use_only() {
        let mut watch = Watch::default();
        let mic = using(true, false, &["Zoom"]);
        watch.update(&mic);
        let seen = watch.take_sighting(&mic).expect("the first sighting");
        assert_eq!(seen.tier, Some(IslandTier::Whisper));
        assert_eq!(seen.short.as_deref(), Some("Mic"));

        watch.update(&mic);
        assert!(
            watch.take_sighting(&mic).is_none(),
            "not again on the next poll"
        );
        watch.update(&PrivacyUse::default());
        watch.update(&mic);
        assert!(
            watch.take_sighting(&mic).is_none(),
            "a blip is not a new start"
        );

        watch.update(&PrivacyUse::default());
        watch.update(&PrivacyUse::default());
        watch.update(&mic);
        assert!(watch.take_sighting(&mic).is_some(), "a real second call is");
    }
}
