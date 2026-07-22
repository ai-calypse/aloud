//! Turning a physical key gesture into a [`Binding`]. Pure and
//! platform-neutral, like [`crate::matcher`].
//!
//! Committing on the first key-down would record "Command" for every chord
//! starting with it, so the gesture accumulates while anything is held and
//! commits on the last release, keeping the most specific candidate seen.

use crate::model::{Binding, Key, Modifiers};
use std::collections::BTreeSet;

/// Accumulates one key gesture into a binding.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Recorder {
    pressed: BTreeSet<Key>,
    /// Best candidate seen during this gesture. Kept rather than recomputed on
    /// release because the keys are gone by then.
    best: Option<Binding>,
}

impl Recorder {
    pub fn key_down(&mut self, key: Key) {
        self.pressed.insert(key);

        // Side-specific keys held alongside the primary. Recording pins to
        // exactly these, keeping Left-Option+Z and Right-Option+Z apart — the
        // reason this is stored and not just the logical `modifiers`.
        let exact_modifiers: BTreeSet<Key> = self
            .pressed
            .iter()
            .copied()
            .filter(|k| *k != key && k.logical_modifier().is_some())
            .collect();
        let modifiers: Modifiers = exact_modifiers
            .iter()
            .filter_map(|k| k.logical_modifier())
            .collect();

        let candidate = if key.logical_modifier().is_some() {
            Binding::ModifierKey {
                key,
                modifiers,
                exact_modifiers,
            }
        } else {
            Binding::Key {
                key,
                modifiers,
                exact_modifiers,
            }
        };

        if self
            .best
            .as_ref()
            .is_none_or(|best| outranks(&candidate, best))
        {
            self.best = Some(candidate);
        }
    }

    /// Records a key coming up. Returns the binding once the last key is
    /// released, and resets ready for the next gesture.
    pub fn key_up(&mut self, key: Key) -> Option<Binding> {
        self.pressed.remove(&key);
        if !self.pressed.is_empty() {
            return None;
        }
        self.best.take()
    }

    /// Drops a half-finished gesture. Used when capture is cancelled, so a
    /// stale held key cannot leak into the next recording.
    pub fn reset(&mut self) {
        self.pressed.clear();
        self.best = None;
    }
}

/// A real key always wins — Cmd+Shift+D means D, and the modifier-only
/// candidates seen on the way are scaffolding. Otherwise, more modifiers wins.
fn outranks(candidate: &Binding, current: &Binding) -> bool {
    let plain = |b: &Binding| matches!(b, Binding::Key { .. });
    match (plain(candidate), plain(current)) {
        (true, false) => true,
        (false, true) => false,
        _ => candidate.specificity() > current.specificity(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Modifier;

    fn mods(list: &[Modifier]) -> Modifiers {
        list.iter().copied().collect()
    }

    fn exact(list: &[Key]) -> BTreeSet<Key> {
        list.iter().copied().collect()
    }

    #[test]
    fn modifier_held_alone_records_as_modifier_key() {
        let mut r = Recorder::default();
        r.key_down(Key::RightOption);
        assert_eq!(
            r.key_up(Key::RightOption),
            Some(Binding::modifier_chord(Key::RightOption, Modifiers::new()))
        );
    }

    /// Committing on the first key-down records "Command" for every chord
    /// starting with Command.
    #[test]
    fn chord_records_the_real_key_not_the_first_modifier() {
        let mut r = Recorder::default();
        r.key_down(Key::LeftCommand);
        r.key_down(Key::LeftShift);
        r.key_down(Key::Char('d'));
        assert_eq!(r.key_up(Key::Char('d')), None, "keys still held");
        assert_eq!(r.key_up(Key::LeftShift), None, "keys still held");
        assert_eq!(
            r.key_up(Key::LeftCommand),
            Some(Binding::Key {
                key: Key::Char('d'),
                modifiers: mods(&[Modifier::Command, Modifier::Shift]),
                exact_modifiers: exact(&[Key::LeftCommand, Key::LeftShift]),
            })
        );
    }

    /// Lifting the letter first is the common case and must give the same answer.
    #[test]
    fn release_order_does_not_change_the_binding() {
        let mut a = Recorder::default();
        for k in [Key::LeftCommand, Key::Char('k')] {
            a.key_down(k);
        }
        a.key_up(Key::Char('k'));
        let first = a.key_up(Key::LeftCommand);

        let mut b = Recorder::default();
        for k in [Key::LeftCommand, Key::Char('k')] {
            b.key_down(k);
        }
        b.key_up(Key::LeftCommand);
        let second = b.key_up(Key::Char('k'));

        assert_eq!(first, second);
    }

    #[test]
    fn extra_modifiers_beat_fewer() {
        let mut r = Recorder::default();
        r.key_down(Key::LeftCommand);
        r.key_down(Key::LeftOption);
        assert_eq!(r.key_up(Key::LeftCommand), None, "Option still held");
        assert_eq!(
            r.key_up(Key::LeftOption),
            Some(Binding::ModifierKey {
                key: Key::LeftOption,
                modifiers: mods(&[Modifier::Command]),
                exact_modifiers: exact(&[Key::LeftCommand]),
            })
        );
    }

    #[test]
    fn side_specific_modifiers_survive() {
        let mut r = Recorder::default();
        r.key_down(Key::RightControl);
        let captured = r.key_up(Key::RightControl).unwrap();
        assert_eq!(captured.key(), Some(Key::RightControl));
    }

    #[test]
    fn function_keys_record_as_plain_keys() {
        let mut r = Recorder::default();
        r.key_down(Key::F(5));
        assert_eq!(
            r.key_up(Key::F(5)),
            Some(Binding::key_chord(Key::F(5), Modifiers::new()))
        );
    }

    #[test]
    fn reset_discards_a_half_finished_gesture() {
        let mut r = Recorder::default();
        r.key_down(Key::LeftCommand);
        r.reset();
        // The stale Command must not reappear as a modifier on the next one.
        r.key_down(Key::Char('j'));
        assert_eq!(
            r.key_up(Key::Char('j')),
            Some(Binding::key_chord(Key::Char('j'), Modifiers::new()))
        );
    }

    /// A gesture is one recording. Two in a row must not bleed together.
    #[test]
    fn recorder_is_reusable_after_a_commit() {
        let mut r = Recorder::default();
        r.key_down(Key::RightOption);
        r.key_up(Key::RightOption);

        // F5, not Escape: Escape is intercepted as the capture bail-out before
        // it ever reaches the recorder.
        r.key_down(Key::F(5));
        assert_eq!(
            r.key_up(Key::F(5)),
            Some(Binding::key_chord(Key::F(5), Modifiers::new()))
        );
    }
}
