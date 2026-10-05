//! Keys the dictation reducer must never see: the volume keys (which macOS
//! delivers as NX_SYSDEFINED, not as key-downs) and the Caps Lock edge. They
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
    /// Caps Lock turned on or off.
    CapsLock(bool),
}

/// What `NSEvent` reads out of an NX_SYSDEFINED media-key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemKey {
    pub key: VolumeKey,
    pub down: bool,
}

/// `NX_SUBTYPE_AUX_CONTROL_BUTTONS`: the media and volume keys.
const SUBTYPE_AUX_CONTROL_BUTTONS: i16 = 8;
const NX_KEYTYPE_SOUND_UP: i64 = 0;
const NX_KEYTYPE_SOUND_DOWN: i64 = 1;
const NX_KEYTYPE_MUTE: i64 = 7;
const NX_KEYSTATE_DOWN: i64 = 0xA;

/// Decodes an NX_SYSDEFINED event's `subtype` and `data1`: the key code is the
/// top half of `data1`, the state byte sits in the bottom half. None for
/// anything that is not a volume key (brightness, play/pause, other subtypes).
pub fn decode_system_key(subtype: i16, data1: i64) -> Option<SystemKey> {
    if subtype != SUBTYPE_AUX_CONTROL_BUTTONS {
        return None;
    }
    let key = match (data1 & 0xFFFF_0000) >> 16 {
        NX_KEYTYPE_SOUND_UP => VolumeKey::Up,
        NX_KEYTYPE_SOUND_DOWN => VolumeKey::Down,
        NX_KEYTYPE_MUTE => VolumeKey::Mute,
        _ => return None,
    };
    Some(SystemKey {
        key,
        down: (data1 & 0xFF00) >> 8 == NX_KEYSTATE_DOWN,
    })
}

/// The "Replace the macOS volume HUD" setting, as the app last applied it.
static REPLACE_VOLUME: AtomicBool = AtomicBool::new(false);
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
                key: VolumeKey::Up,
                down: true
            })
        );
    }

    /// Regression guard: brightness (2, 3), play/pause (16) and every other
    /// subtype must not be mistaken for a volume key and swallowed.
    #[test]
    fn brightness_media_and_other_subtypes_are_not_volume() {
        assert_eq!(decode_system_key(8, data1(2, 0xA, false)), None);
        assert_eq!(decode_system_key(8, data1(16, 0xA, false)), None);
        assert_eq!(decode_system_key(7, data1(0, 0xA, false)), None);
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
