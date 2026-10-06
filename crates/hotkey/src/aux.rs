//! Keys the dictation reducer must never see: the volume and brightness keys
//! (which macOS delivers as NX_SYSDEFINED, not as key-downs, and on some
//! keyboards as keycodes 144/145) and the Caps Lock edge. They
//! leave the tap on their own channel, so a press of either cannot reach the
//! matcher, the capture recorder or the typing signal.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKey {
    Up,
    Down,
    Mute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrightnessKey {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuxEvent {
    /// A volume key went down (auto-repeats included).
    Volume {
        key: VolumeKey,
        /// Option+Shift: macOS moves a quarter step instead of a full one.
        fine: bool,
        /// Whether the tap ate the key. When it did not, macOS draws its own
        /// HUD and the consumer must not move the volume as well.
        swallowed: bool,
    },
    /// A brightness key went down (auto-repeats included).
    Brightness {
        key: BrightnessKey,
        /// Option+Shift, as for the volume.
        fine: bool,
        /// As for [`AuxEvent::Volume`]: when false, macOS moves the brightness
        /// and draws its own HUD, and the consumer must not move it as well.
        swallowed: bool,
    },
    /// Caps Lock turned on or off.
    CapsLock(bool),
}

/// The keys a HUD answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudKey {
    Volume(VolumeKey),
    Brightness(BrightnessKey),
}

/// What `NSEvent` reads out of an NX_SYSDEFINED media-key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemKey {
    pub key: HudKey,
    pub down: bool,
}

/// `NX_SUBTYPE_AUX_CONTROL_BUTTONS`: the media and volume keys.
const SUBTYPE_AUX_CONTROL_BUTTONS: i16 = 8;
const NX_KEYTYPE_SOUND_UP: i64 = 0;
const NX_KEYTYPE_SOUND_DOWN: i64 = 1;
const NX_KEYTYPE_BRIGHTNESS_UP: i64 = 2;
const NX_KEYTYPE_BRIGHTNESS_DOWN: i64 = 3;
const NX_KEYTYPE_MUTE: i64 = 7;
const NX_KEYSTATE_DOWN: i64 = 0xA;

/// Decodes an NX_SYSDEFINED event's `subtype` and `data1`: the key code is the
/// top half of `data1`, the state byte sits in the bottom half. None for
/// anything that is not a volume or brightness key (play/pause, other subtypes).
pub fn decode_system_key(subtype: i16, data1: i64) -> Option<SystemKey> {
    if subtype != SUBTYPE_AUX_CONTROL_BUTTONS {
        return None;
    }
    let key = match (data1 & 0xFFFF_0000) >> 16 {
        NX_KEYTYPE_SOUND_UP => HudKey::Volume(VolumeKey::Up),
        NX_KEYTYPE_SOUND_DOWN => HudKey::Volume(VolumeKey::Down),
        NX_KEYTYPE_MUTE => HudKey::Volume(VolumeKey::Mute),
        NX_KEYTYPE_BRIGHTNESS_UP => HudKey::Brightness(BrightnessKey::Up),
        NX_KEYTYPE_BRIGHTNESS_DOWN => HudKey::Brightness(BrightnessKey::Down),
        _ => return None,
    };
    Some(SystemKey {
        key,
        down: (data1 & 0xFF00) >> 8 == NX_KEYSTATE_DOWN,
    })
}

/// Brightness as an ordinary key-down: the keycodes MonitorControl's
/// MediaKeyTap maps to "brightness up/down media key" next to the
/// NX_SYSDEFINED path (`functionKeyCodeToMediaKey`). The Mac keyboard that
/// sends these instead of NX_SYSDEFINED is not known here; both are decoded
/// so either way the key is seen. Neither has a `kVK_` constant, so no typing
/// key shares them. (F14/F15, which MediaKeyTap also maps for third-party
/// keyboards, are real keys and are left alone.)
pub fn brightness_keycode(code: i64) -> Option<BrightnessKey> {
    match code {
        144 => Some(BrightnessKey::Up),
        145 => Some(BrightnessKey::Down),
        _ => None,
    }
}

/// The "Replace the macOS volume and brightness HUDs" setting, as the app last
/// applied it. One switch, two flags: brightness has its own `controllable`.
static REPLACE_VOLUME: AtomicBool = AtomicBool::new(false);
static REPLACE_BRIGHTNESS: AtomicBool = AtomicBool::new(false);
/// Whether the built-in display is the one to move. Asked on the tap thread
/// for each brightness key, so it must be cheap (no display I/O).
static BRIGHTNESS_PROBE: OnceLock<fn() -> bool> = OnceLock::new();
/// Whether the default output has a volume to move. Cleared by the consumer
/// when it finds one that has not (HDMI, some USB interfaces): swallowing a
/// key nothing can act on would leave it dead.
static CONTROLLABLE: AtomicBool = AtomicBool::new(true);
static CAPS_ON: AtomicBool = AtomicBool::new(false);
static SINK: OnceLock<Sender<AuxEvent>> = OnceLock::new();

/// Eat the volume keys and let the app draw the HUD, instead of macOS. Read on
/// the tap thread for every volume key, so toggling it needs no restart.
pub fn set_replace_volume(on: bool) {
    REPLACE_VOLUME.store(on, Ordering::Relaxed);
}

pub fn set_volume_controllable(yes: bool) {
    CONTROLLABLE.store(yes, Ordering::Relaxed);
}

/// As [`set_replace_volume`], for the brightness keys.
pub fn set_replace_brightness(on: bool) {
    REPLACE_BRIGHTNESS.store(on, Ordering::Relaxed);
}

/// Says whether there is a built-in display to move. Unset means no: a tap
/// with nothing to ask must never eat a key.
pub fn set_brightness_probe(probe: fn() -> bool) {
    let _ = BRIGHTNESS_PROBE.set(probe);
}

pub(crate) fn brightness_controllable() -> bool {
    BRIGHTNESS_PROBE.get().is_some_and(|probe| probe())
}

/// Whether a brightness key should be eaten. `passthrough` is Option alone
/// (opens Displays settings) or Control (macOS aims it at an external
/// display): both are left to macOS. `controllable` is asked last, so the
/// display is not queried while replacement is off.
pub(crate) fn swallows_brightness(passthrough: bool, controllable: impl FnOnce() -> bool) -> bool {
    REPLACE_BRIGHTNESS.load(Ordering::Relaxed) && !passthrough && controllable()
}

/// Whether a volume key should be eaten. Option alone is left to macOS: it
/// opens Sound settings, which a replaced HUD must not break.
pub(crate) fn swallows_volume(option_without_shift: bool) -> bool {
    REPLACE_VOLUME.load(Ordering::Relaxed)
        && CONTROLLABLE.load(Ordering::Relaxed)
        && !option_without_shift
}

/// The stream of [`AuxEvent`]s. Take it once, before the tap starts: until
/// then nothing is queued, so a build that never listens never grows a backlog.
pub fn events() -> Option<Receiver<AuxEvent>> {
    let (tx, rx) = channel();
    SINK.set(tx).ok().map(|()| rx)
}

pub(crate) fn emit(event: AuxEvent) {
    if let Some(tx) = SINK.get() {
        let _ = tx.send(event);
    }
}

/// Caps Lock arrives as a flagsChanged on press and again on release on some
/// keyboards: only a change of state is an edge.
pub(crate) fn caps_edge(on: bool) -> Option<AuxEvent> {
    (CAPS_ON.swap(on, Ordering::Relaxed) != on).then_some(AuxEvent::CapsLock(on))
}

pub(crate) fn seed_caps(on: bool) {
    CAPS_ON.store(on, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data1(key: i64, state: i64, repeat: bool) -> i64 {
        (key << 16) | (state << 8) | i64::from(repeat)
    }

    #[test]
    fn decodes_the_three_volume_keys_down_and_up() {
        for (code, key) in [
            (0, VolumeKey::Up),
            (1, VolumeKey::Down),
            (7, VolumeKey::Mute),
        ] {
            let key = HudKey::Volume(key);
            assert_eq!(
                decode_system_key(8, data1(code, 0xA, false)),
                Some(SystemKey { key, down: true })
            );
            assert_eq!(
                decode_system_key(8, data1(code, 0xB, false)),
                Some(SystemKey { key, down: false })
            );
        }
    }

    /// A held key repeats with the low bit set; each repeat is still a press.
    #[test]
    fn an_auto_repeat_is_still_a_key_down() {
        assert_eq!(
            decode_system_key(8, data1(0, 0xA, true)),
            Some(SystemKey {
                key: HudKey::Volume(VolumeKey::Up),
                down: true
            })
        );
    }

    #[test]
    fn decodes_both_brightness_keys_down_and_up() {
        for (code, key) in [(2, BrightnessKey::Up), (3, BrightnessKey::Down)] {
            let key = HudKey::Brightness(key);
            assert_eq!(
                decode_system_key(8, data1(code, 0xA, false)),
                Some(SystemKey { key, down: true })
            );
            assert_eq!(
                decode_system_key(8, data1(code, 0xB, true)),
                Some(SystemKey { key, down: false })
            );
        }
    }

    /// Regression guard: play/pause (16), the keyboard backlight (21, 22) and
    /// every other subtype must not be mistaken for a HUD key and swallowed.
    #[test]
    fn other_media_keys_and_subtypes_are_not_hud_keys() {
        for code in [4, 5, 6, 16, 17, 18, 19, 20, 21, 22, 23] {
            assert_eq!(decode_system_key(8, data1(code, 0xA, false)), None);
        }
        assert_eq!(decode_system_key(7, data1(0, 0xA, false)), None);
        assert_eq!(decode_system_key(7, data1(2, 0xA, false)), None);
    }

    #[test]
    fn brightness_keydown_keycodes_are_only_144_and_145() {
        assert_eq!(brightness_keycode(144), Some(BrightnessKey::Up));
        assert_eq!(brightness_keycode(145), Some(BrightnessKey::Down));
        // F14/F15 and the arrows around them are ordinary keys.
        for code in [107, 113, 143, 146, 126, 125] {
            assert_eq!(brightness_keycode(code), None);
        }
    }

    /// Off by default, left to macOS when the built-in display is not the one
    /// in use (external main display, lid closed), and the probe is not even
    /// asked while replacement is off.
    #[test]
    fn brightness_is_never_swallowed_unless_replacement_is_on_and_controllable() {
        set_replace_brightness(false);
        assert!(!swallows_brightness(false, || panic!("asked while off")));
        set_replace_brightness(true);
        assert!(swallows_brightness(false, || true));
        assert!(
            !swallows_brightness(false, || false),
            "external main display"
        );
        assert!(!swallows_brightness(true, || true), "Option opens Displays");
        set_replace_brightness(false);
    }

    #[test]
    fn caps_lock_reports_changes_not_repeats() {
        seed_caps(false);
        assert_eq!(caps_edge(true), Some(AuxEvent::CapsLock(true)));
        assert_eq!(caps_edge(true), None, "the release of the same toggle");
        assert_eq!(caps_edge(false), Some(AuxEvent::CapsLock(false)));
    }

    /// The setting is off by default: a tap must never eat a key unasked.
    #[test]
    fn volume_is_never_swallowed_unless_replacement_is_on_and_controllable() {
        set_replace_volume(false);
        assert!(!swallows_volume(false));
        set_replace_volume(true);
        assert!(swallows_volume(false));
        assert!(!swallows_volume(true), "Option opens Sound settings");
        set_volume_controllable(false);
        assert!(!swallows_volume(false));
        set_volume_controllable(true);
        set_replace_volume(false);
    }
}
