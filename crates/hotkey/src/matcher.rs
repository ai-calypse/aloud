//! Pure key-state reducer, ported from `ShortcutMatcher.swift`.
//!
//! No OS calls live here: backends feed raw key up/down in, and this decides
//! which roles activate, in what order, and whether the key event should be
//! swallowed. That keeps the tricky part unit-testable on any platform.

use crate::model::{Binding, Config, Event, Key, Modifier, Modifiers, Role};
use std::collections::BTreeSet;

/// Raw input from a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    KeyDown(Key),
    KeyUp(Key),
    /// Backend lost sync (tap re-enabled, focus change): clear all held keys.
    Reset,
}

/// Whether the event should be swallowed or passed to the focused app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consume {
    /// Swallow it: it drove a binding, so the app below must not also see it.
    Yes,
    No,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct State {
    pressed: BTreeSet<Key>,
    /// Roles currently held down. One-shots never appear here — they have no
    /// duration — so they are tracked in `fired` instead.
    active: BTreeSet<Role>,
    /// One-shots that have fired and are still held, so they fire once per
    /// press rather than on every key repeat.
    fired: BTreeSet<Role>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub events: Vec<Event>,
    pub consume: Consume,
}

impl State {
    pub fn is_active(&self, role: Role) -> bool {
        self.active.contains(&role)
    }

    fn modifiers(&self) -> Modifiers {
        self.pressed
            .iter()
            .filter_map(|k| k.logical_modifier())
            .collect()
    }

    /// Feeds one input event through, returning what fired.
    pub fn reduce(&mut self, input: Input, config: &Config) -> Outcome {
        match input {
            Input::Reset => {
                self.pressed.clear();
                self.fired.clear();
                let events = self.sync_active(config);
                Outcome {
                    events,
                    consume: Consume::No,
                }
            }
            Input::KeyDown(key) | Input::KeyUp(key) => {
                let down = matches!(input, Input::KeyDown(_));

                // Escape cancels an in-flight session and is swallowed only
                // then — otherwise Escape must reach the focused app.
                if key == Key::Escape && down && !self.active.is_empty() {
                    let events = std::iter::once(Event::Cancelled)
                        .chain(self.active.iter().copied().map(Event::Deactivated))
                        .collect();
                    self.active.clear();
                    return Outcome {
                        events,
                        consume: Consume::Yes,
                    };
                }

                // A binding is relevant if it was armed before or after this
                // key moved: a key-up must be swallowed by the binding the
                // matching key-down triggered.
                let armed_before = self.relevant(key, config);
                if down {
                    self.pressed.insert(key);
                } else {
                    self.pressed.remove(&key);
                }
                let armed_after = self.relevant(key, config);

                let events = self.sync_active(config);
                Outcome {
                    events,
                    consume: if armed_before || armed_after {
                        Consume::Yes
                    } else {
                        Consume::No
                    },
                }
            }
        }
    }

    /// Recomputes which roles are active and emits the transitions.
    ///
    /// Activations are ordered most-specific first and deactivations
    /// least-specific first, matching the Swift ordering so that overlapping
    /// bindings (Cmd vs Cmd+Shift) resolve the same way.
    fn sync_active(&mut self, config: &Config) -> Vec<Event> {
        let mut activated: Vec<(Role, usize)> = Vec::new();
        let mut deactivated: Vec<(Role, usize)> = Vec::new();

        for (role, binding) in config.roles() {
            let now = self.binding_is_active(binding);

            if role.is_one_shot() {
                // Fire on the leading edge only; no deactivation edge. `fired`
                // tracks the press so key repeats don't re-trigger it.
                match (self.fired.contains(&role), now) {
                    (false, true) => {
                        activated.push((role, binding.specificity()));
                        self.fired.insert(role);
                    }
                    (true, false) => {
                        self.fired.remove(&role);
                    }
                    _ => {}
                }
                continue;
            }

            match (self.active.contains(&role), now) {
                (false, true) => {
                    activated.push((role, binding.specificity()));
                    self.active.insert(role);
                }
                (true, false) => {
                    deactivated.push((role, binding.specificity()));
                    self.active.remove(&role);
                }
                _ => {}
            }
        }

        activated.sort_by_key(|(_, spec)| std::cmp::Reverse(*spec));
        deactivated.sort_by_key(|(_, spec)| *spec);

        activated
            .into_iter()
            .map(|(r, _)| Event::Activated(r))
            .chain(deactivated.into_iter().map(|(r, _)| Event::Deactivated(r)))
            .collect()
    }

    fn binding_is_active(&self, binding: &Binding) -> bool {
        let Some(key) = binding.key() else {
            return false;
        };
        if !self.pressed.contains(&key) {
            return false;
        }

        let required = binding.modifiers().cloned().unwrap_or_default();
        let held = self.modifiers();

        // Every required modifier must be held. Extra modifiers are only
        // tolerated for plain-key bindings; a modifier-only binding demands an
        // exact match, so Right Option alone fires but Cmd+Right Option does
        // not — otherwise every Cmd chord would start dictation.
        match binding {
            Binding::ModifierKey { .. } => {
                let extra: Modifiers = held
                    .difference(&required)
                    .copied()
                    .filter(|m| Some(*m) != key.logical_modifier())
                    .collect();
                required.is_subset(&held) && extra.is_empty()
            }
            _ => required.is_subset(&held),
        }
    }

    /// Whether `key` participates in any configured binding — used to decide
    /// whether to swallow the event.
    fn relevant(&self, key: Key, config: &Config) -> bool {
        config.roles().any(|(_, binding)| {
            if binding.key() == Some(key) {
                return self.binding_is_active(binding);
            }
            // A modifier that a binding requires is only swallowed while that
            // binding is actually firing.
            match key.logical_modifier() {
                Some(m) => {
                    binding.modifiers().is_some_and(|mods| mods.contains(&m))
                        && self.binding_is_active(binding)
                }
                None => false,
            }
        })
    }
}

/// Convenience: `Modifiers` from a slice, for config building and tests.
pub fn modifiers(list: &[Modifier]) -> Modifiers {
    list.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hold_right_option() -> Config {
        Config {
            hold: Some(Binding::ModifierKey {
                key: Key::RightOption,
                modifiers: Modifiers::new(),
            }),
            ..Default::default()
        }
    }

    fn hold_cmd_d() -> Config {
        Config {
            hold: Some(Binding::Key {
                key: Key::Char('d'),
                modifiers: modifiers(&[Modifier::Command]),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn modifier_only_hold_activates_and_deactivates() {
        let config = hold_right_option();
        let mut state = State::default();

        let down = state.reduce(Input::KeyDown(Key::RightOption), &config);
        assert_eq!(down.events, vec![Event::Activated(Role::Hold)]);
        assert_eq!(down.consume, Consume::Yes);
        assert!(state.is_active(Role::Hold));

        let up = state.reduce(Input::KeyUp(Key::RightOption), &config);
        assert_eq!(up.events, vec![Event::Deactivated(Role::Hold)]);
        assert!(!state.is_active(Role::Hold));
    }

    /// The whole reason this crate exists: a bare modifier hold must not fire
    /// when it is part of a normal chord like Cmd+Right Option.
    #[test]
    fn modifier_only_hold_ignores_extra_modifiers() {
        let config = hold_right_option();
        let mut state = State::default();

        state.reduce(Input::KeyDown(Key::LeftCommand), &config);
        let out = state.reduce(Input::KeyDown(Key::RightOption), &config);

        assert!(
            out.events.is_empty(),
            "Cmd+RightOption must not start dictation"
        );
        assert!(!state.is_active(Role::Hold));
    }

    #[test]
    fn key_binding_requires_its_modifier() {
        let config = hold_cmd_d();
        let mut state = State::default();

        // 'd' alone does nothing and must reach the focused app.
        let bare = state.reduce(Input::KeyDown(Key::Char('d')), &config);
        assert!(bare.events.is_empty());
        assert_eq!(bare.consume, Consume::No, "plain typing must pass through");
        state.reduce(Input::KeyUp(Key::Char('d')), &config);

        state.reduce(Input::KeyDown(Key::LeftCommand), &config);
        let out = state.reduce(Input::KeyDown(Key::Char('d')), &config);
        assert_eq!(out.events, vec![Event::Activated(Role::Hold)]);
        assert_eq!(out.consume, Consume::Yes);
    }

    #[test]
    fn key_up_is_consumed_by_the_binding_it_triggered() {
        let config = hold_cmd_d();
        let mut state = State::default();

        state.reduce(Input::KeyDown(Key::LeftCommand), &config);
        state.reduce(Input::KeyDown(Key::Char('d')), &config);

        let up = state.reduce(Input::KeyUp(Key::Char('d')), &config);
        assert_eq!(up.events, vec![Event::Deactivated(Role::Hold)]);
        assert_eq!(
            up.consume,
            Consume::Yes,
            "the app must not see a stray key-up"
        );
    }

    #[test]
    fn one_shot_fires_once_per_press() {
        let config = Config {
            copy_again: Some(Binding::Key {
                key: Key::Char('v'),
                modifiers: modifiers(&[Modifier::Command, Modifier::Shift]),
            }),
            ..Default::default()
        };
        let mut state = State::default();

        state.reduce(Input::KeyDown(Key::LeftCommand), &config);
        state.reduce(Input::KeyDown(Key::LeftShift), &config);
        let first = state.reduce(Input::KeyDown(Key::Char('v')), &config);
        assert_eq!(first.events, vec![Event::Activated(Role::CopyAgain)]);

        // Still held: must not repeat, and must emit no deactivation on release.
        let held = state.reduce(Input::KeyDown(Key::Char('v')), &config);
        assert!(
            held.events.is_empty(),
            "one-shot must not repeat while held"
        );
        let up = state.reduce(Input::KeyUp(Key::Char('v')), &config);
        assert!(up.events.is_empty(), "one-shot has no deactivation edge");
    }

    #[test]
    fn escape_cancels_active_session() {
        let config = hold_right_option();
        let mut state = State::default();
        state.reduce(Input::KeyDown(Key::RightOption), &config);

        let out = state.reduce(Input::KeyDown(Key::Escape), &config);
        assert_eq!(
            out.events,
            vec![Event::Cancelled, Event::Deactivated(Role::Hold)]
        );
        assert_eq!(out.consume, Consume::Yes);
        assert!(!state.is_active(Role::Hold));
    }

    #[test]
    fn escape_passes_through_when_idle() {
        let config = hold_right_option();
        let mut state = State::default();

        let out = state.reduce(Input::KeyDown(Key::Escape), &config);
        assert!(out.events.is_empty());
        assert_eq!(
            out.consume,
            Consume::No,
            "Escape must reach the focused app"
        );
    }

    /// More-specific bindings activate first so a Cmd+Shift chord wins over a
    /// bare Cmd one.
    #[test]
    fn activations_are_ordered_most_specific_first() {
        let config = Config {
            hold: Some(Binding::Key {
                key: Key::Char('d'),
                modifiers: modifiers(&[Modifier::Command]),
            }),
            ask: Some(Binding::Key {
                key: Key::Char('d'),
                modifiers: modifiers(&[Modifier::Command, Modifier::Shift]),
            }),
            ..Default::default()
        };
        let mut state = State::default();

        state.reduce(Input::KeyDown(Key::LeftCommand), &config);
        state.reduce(Input::KeyDown(Key::LeftShift), &config);
        let out = state.reduce(Input::KeyDown(Key::Char('d')), &config);

        assert_eq!(
            out.events,
            vec![Event::Activated(Role::Ask), Event::Activated(Role::Hold)],
            "Cmd+Shift+D (2 modifiers) must precede Cmd+D (1)"
        );
    }

    #[test]
    fn reset_releases_everything() {
        let config = hold_right_option();
        let mut state = State::default();
        state.reduce(Input::KeyDown(Key::RightOption), &config);

        let out = state.reduce(Input::Reset, &config);
        assert_eq!(out.events, vec![Event::Deactivated(Role::Hold)]);
        assert!(!state.is_active(Role::Hold));
    }

    #[test]
    fn either_side_satisfies_a_logical_modifier() {
        let config = hold_cmd_d();
        let mut state = State::default();

        state.reduce(Input::KeyDown(Key::RightCommand), &config);
        let out = state.reduce(Input::KeyDown(Key::Char('d')), &config);
        assert_eq!(out.events, vec![Event::Activated(Role::Hold)]);
    }

    #[test]
    fn disabled_bindings_never_fire() {
        let mut state = State::default();
        let out = state.reduce(Input::KeyDown(Key::RightOption), &Config::default());
        assert!(out.events.is_empty());
        assert_eq!(out.consume, Consume::No);
    }
}
