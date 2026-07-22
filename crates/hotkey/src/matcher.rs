//! Pure key-state reducer. No OS calls: backends feed raw key up/down in, and
//! this decides which roles activate, in what order, and whether the event is
//! swallowed — so the tricky part is unit-testable on any platform.

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
    /// Roles currently held down. One-shots have no duration, so they live in
    /// `fired` instead.
    active: BTreeSet<Role>,
    /// One-shots that fired and are still held, so key repeats don't re-trigger.
    fired: BTreeSet<Role>,
    /// A session still running after its key was released. Escape must reach
    /// that window: `active` empties the instant Hold is lifted — exactly when
    /// the slow part begins — and Toggle is never in `active` at all.
    busy: bool,
    /// Roles Escape cancelled whose keys are still physically down. Clearing
    /// `active` alone is not enough: the binding is still satisfied, so the next
    /// event re-derives it and starts a second recording.
    suppressed: BTreeSet<Role>,
    /// Key-downs that were swallowed, so the matching key-up is swallowed too.
    /// Without it a Cmd+D binding leaves the focused app holding a Command it
    /// never saw released.
    consumed: BTreeSet<Key>,
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

    /// Whether a session is in flight. Drives nothing but Escape.
    pub fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
    }

    /// The modifiers held right now, so a plain dictation can be told from one
    /// the user marked as a rewrite.
    pub fn modifiers(&self) -> Modifiers {
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
                // A key held when the tap desynced would otherwise leave an
                // entry here that swallows some unrelated later key-up.
                self.consumed.clear();
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
                if key == Key::Escape && down && (!self.active.is_empty() || self.busy) {
                    let events = std::iter::once(Event::Cancelled)
                        .chain(self.active.iter().copied().map(Event::Deactivated))
                        .collect();
                    // Moved rather than cleared: the keys are still down, so
                    // these roles stay suppressed until a real release.
                    self.suppressed.append(&mut self.active);
                    // Swallow the key-up too, or the app below sees an Escape
                    // it never saw pressed.
                    self.consumed.insert(key);
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
                // A key-up is swallowed iff its key-down was, so every modifier
                // the app saw pressed it also sees released. An unmatched key-up
                // (tap started mid-press) passes through, which is correct.
                let consume = if down {
                    if armed_before || armed_after {
                        self.consumed.insert(key);
                        Consume::Yes
                    } else {
                        Consume::No
                    }
                } else if self.consumed.remove(&key) {
                    Consume::Yes
                } else {
                    Consume::No
                };
                Outcome { events, consume }
            }
        }
    }

    /// Recomputes which roles are active and emits the transitions, ordered
    /// most-specific first on activation and least-specific first on
    /// deactivation so overlapping bindings (Cmd vs Cmd+Shift) resolve stably.
    fn sync_active(&mut self, config: &Config) -> Vec<Event> {
        let mut activated: Vec<(Role, usize)> = Vec::new();
        let mut deactivated: Vec<(Role, usize)> = Vec::new();

        // Every role, not just the bound ones: a role whose binding was cleared
        // mid-hold is absent from `config.roles()` and so never deactivates,
        // leaving the mic open with nothing left to stop it.
        for role in Role::ALL {
            let binding = config.binding(role);
            let now = self.binding_is_active(binding);

            if role.is_one_shot() {
                // Leading edge only, no deactivation edge; `fired` tracks the
                // press so key repeats don't re-trigger it.
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
                // Escape cancelled this role and its key is still down; stay out
                // or the Escape key-up alone starts a second recording.
                (false, true) if self.suppressed.contains(&role) => {}
                (false, true) => {
                    activated.push((role, binding.specificity()));
                    self.active.insert(role);
                }
                (true, false) => {
                    deactivated.push((role, binding.specificity()));
                    self.active.remove(&role);
                }
                // The real release: re-arm. The only arm a suppressed role can
                // reach, so clearing anywhere else would latch it.
                (false, false) => {
                    self.suppressed.remove(&role);
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

        // Modifier-only bindings demand an exact match: Right Option alone
        // fires, Cmd+Right Option does not, or every Cmd chord starts dictation.
        let logical_ok = match binding {
            Binding::ModifierKey { .. } => {
                let extra: Modifiers = held
                    .difference(&required)
                    .copied()
                    .filter(|m| Some(*m) != key.logical_modifier())
                    .collect();
                required.is_subset(&held) && extra.is_empty()
            }
            // A bare keystroke must reject extras or a Space binding eats
            // Cmd+Space before Spotlight sees it. With a modifier required,
            // extras pass and specificity ordering resolves Cmd+D vs Cmd+Shift+D.
            _ => required.is_subset(&held) && (!required.is_empty() || held.is_empty()),
        };
        if !logical_ok {
            return false;
        }

        // A recorded chord pins to sides: held modifiers (bar the primary key)
        // must equal the captured set, so Left-Option rejects Right Option. An
        // empty set — every preset — skips this and matches either side.
        match binding.exact_modifiers() {
            Some(exact) if !exact.is_empty() => {
                let held_keys: BTreeSet<Key> = self
                    .pressed
                    .iter()
                    .copied()
                    .filter(|k| *k != key && k.logical_modifier().is_some())
                    .collect();
                &held_keys == exact
            }
            _ => true,
        }
    }

    /// Whether `key` participates in any configured binding — decides swallowing.
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
            hold: Some(Binding::modifier_chord(Key::RightOption, Modifiers::new())),
            ..Default::default()
        }
    }

    fn hold_cmd_d() -> Config {
        Config {
            hold: Some(Binding::key_chord(
                Key::Char('d'),
                modifiers(&[Modifier::Command]),
            )),
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

    /// A bare modifier hold must not fire as part of a normal chord.
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

    /// Documents the rewrite-hotkey loop: our own synthetic Cmd+C tears down
    /// the hold and its release restarts it, forever. The matcher is right —
    /// Cmd+Option is not Option — so the fix belongs at the tap, which must not
    /// feed our own keystrokes back to us.
    #[test]
    fn a_foreign_modifier_tears_down_a_modifier_only_hold() {
        let config = hold_right_option();
        let mut state = State::default();

        let down = state.reduce(Input::KeyDown(Key::RightOption), &config);
        assert_eq!(down.events, vec![Event::Activated(Role::Hold)]);

        // What enigo's Cmd+C looks like arriving at our own tap.
        let cmd_down = state.reduce(Input::KeyDown(Key::LeftCommand), &config);
        assert_eq!(
            cmd_down.events,
            vec![Event::Deactivated(Role::Hold)],
            "a synthetic Cmd must not be able to end a hold the user is still holding"
        );

        let cmd_up = state.reduce(Input::KeyUp(Key::LeftCommand), &config);
        assert_eq!(
            cmd_up.events,
            vec![Event::Activated(Role::Hold)],
            "...and releasing it restarts the hold, which is the other half of the loop"
        );
    }

    #[test]
    fn key_binding_requires_its_modifier() {
        let config = hold_cmd_d();
        let mut state = State::default();

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
            copy_again: Some(Binding::key_chord(
                Key::Char('v'),
                modifiers(&[Modifier::Command, Modifier::Shift]),
            )),
            ..Default::default()
        };
        let mut state = State::default();

        state.reduce(Input::KeyDown(Key::LeftCommand), &config);
        state.reduce(Input::KeyDown(Key::LeftShift), &config);
        let first = state.reduce(Input::KeyDown(Key::Char('v')), &config);
        assert_eq!(first.events, vec![Event::Activated(Role::CopyAgain)]);

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

    /// Regression: `active` empties on key release, so Escape did nothing for
    /// the whole (up to 80s) transcription round trip.
    #[test]
    fn escape_cancels_after_the_key_is_released() {
        let config = hold_right_option();
        let mut state = State::default();
        state.reduce(Input::KeyDown(Key::RightOption), &config);
        state.reduce(Input::KeyUp(Key::RightOption), &config);
        assert!(!state.is_active(Role::Hold), "key released, work in flight");

        state.set_busy(true);
        let out = state.reduce(Input::KeyDown(Key::Escape), &config);
        assert_eq!(out.events, vec![Event::Cancelled]);
        assert_eq!(
            out.consume,
            Consume::Yes,
            "the cancelling Escape must not also reach the focused app"
        );
    }

    /// Toggle is a one-shot and never enters `active`, so without `busy` it was
    /// uncancellable for its entire life.
    #[test]
    fn escape_cancels_a_toggle_recording() {
        let config = hold_right_option();
        let mut state = State::default();
        state.set_busy(true);

        let out = state.reduce(Input::KeyDown(Key::Escape), &config);
        assert_eq!(out.events, vec![Event::Cancelled]);
        assert_eq!(out.consume, Consume::Yes);
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

    /// More-specific bindings activate first, so Cmd+Shift wins over bare Cmd.
    #[test]
    fn activations_are_ordered_most_specific_first() {
        let config = Config {
            hold: Some(Binding::key_chord(
                Key::Char('d'),
                modifiers(&[Modifier::Command]),
            )),
            ask: Some(Binding::key_chord(
                Key::Char('d'),
                modifiers(&[Modifier::Command, Modifier::Shift]),
            )),
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

    /// `exact_modifiers`: a recorded chord fires only on the side it was
    /// captured with, where a logically-built Option+Z takes either.
    #[test]
    fn a_recorded_chord_pins_to_its_modifier_side() {
        let config = Config {
            hold: Some(Binding::Key {
                key: Key::Char('z'),
                modifiers: modifiers(&[Modifier::Option]),
                exact_modifiers: [Key::LeftOption].into_iter().collect(),
            }),
            ..Default::default()
        };
        let mut state = State::default();

        state.reduce(Input::KeyDown(Key::RightOption), &config);
        let wrong = state.reduce(Input::KeyDown(Key::Char('z')), &config);
        assert!(
            wrong.events.is_empty(),
            "Right Option must not fire a Left-Option-bound chord"
        );
        state.reduce(Input::KeyUp(Key::Char('z')), &config);
        state.reduce(Input::KeyUp(Key::RightOption), &config);

        state.reduce(Input::KeyDown(Key::LeftOption), &config);
        let right = state.reduce(Input::KeyDown(Key::Char('z')), &config);
        assert_eq!(right.events, vec![Event::Activated(Role::Hold)]);
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

    /// Regression: the dictate key is still down after Escape, so its key-up
    /// re-derived the binding and the cancel produced an extra utterance.
    #[test]
    fn escape_does_not_restart_while_the_hold_key_is_still_down() {
        let config = hold_right_option();
        let mut state = State::default();
        state.reduce(Input::KeyDown(Key::RightOption), &config);
        state.reduce(Input::KeyDown(Key::Escape), &config);

        let esc_up = state.reduce(Input::KeyUp(Key::Escape), &config);
        assert!(esc_up.events.is_empty(), "cancel must not start a session");

        let opt_up = state.reduce(Input::KeyUp(Key::RightOption), &config);
        assert!(opt_up.events.is_empty(), "and the release must not dictate");

        // A real release re-arms: cancelling must not disable the key.
        let again = state.reduce(Input::KeyDown(Key::RightOption), &config);
        assert_eq!(again.events, vec![Event::Activated(Role::Hold)]);
    }

    /// Regression: iterating only *bound* roles left a role cleared mid-hold
    /// active forever — the mic ran until relaunch.
    #[test]
    fn clearing_a_binding_mid_hold_still_deactivates() {
        let config = hold_right_option();
        let mut state = State::default();
        state.reduce(Input::KeyDown(Key::RightOption), &config);

        let cleared = Config {
            hold: Some(Binding::Disabled),
            ..Default::default()
        };
        let up = state.reduce(Input::KeyUp(Key::RightOption), &cleared);
        assert_eq!(up.events, vec![Event::Deactivated(Role::Hold)]);
        assert!(!state.is_active(Role::Hold));

        // ...and rebinding is back in phase rather than one edge behind.
        let re = state.reduce(Input::KeyDown(Key::RightOption), &config);
        assert_eq!(re.events, vec![Event::Activated(Role::Hold)]);
    }

    /// Regression: an empty `required` set made the subset test vacuously true,
    /// so a Space binding swallowed Cmd+Space before Spotlight saw it.
    #[test]
    fn a_bare_key_binding_ignores_chords() {
        let config = Config {
            hold: Some(Binding::key_chord(Key::Space, Modifiers::new())),
            ..Default::default()
        };
        let mut state = State::default();
        state.reduce(Input::KeyDown(Key::LeftCommand), &config);

        let out = state.reduce(Input::KeyDown(Key::Space), &config);
        assert!(out.events.is_empty(), "Cmd+Space must not start dictation");
        assert_eq!(
            out.consume,
            Consume::No,
            "and Spotlight must still see Cmd+Space"
        );
    }

    /// Regression: Cmd's key-down passed through but its key-up was swallowed,
    /// leaving the focused app holding a Command it never saw released.
    #[test]
    fn a_modifier_the_app_saw_pressed_is_not_swallowed_on_release() {
        let config = hold_cmd_d();
        let mut state = State::default();

        let cmd_down = state.reduce(Input::KeyDown(Key::LeftCommand), &config);
        assert_eq!(cmd_down.consume, Consume::No, "the app saw Command pressed");
        state.reduce(Input::KeyDown(Key::Char('d')), &config);

        let cmd_up = state.reduce(Input::KeyUp(Key::LeftCommand), &config);
        assert_eq!(cmd_up.events, vec![Event::Deactivated(Role::Hold)]);
        assert_eq!(
            cmd_up.consume,
            Consume::No,
            "...so it must also see Command released"
        );

        let d_up = state.reduce(Input::KeyUp(Key::Char('d')), &config);
        assert_eq!(
            d_up.consume,
            Consume::Yes,
            "d's key-down was swallowed, so its key-up must be too"
        );
    }

    /// Regression: capture swallows every key while armed, so a modifier held
    /// at Record time never delivered its key-up. The phantom then failed the
    /// exact-match rule and the dictate key was dead until relaunch.
    #[test]
    fn reset_clears_a_key_capture_swallowed() {
        let config = hold_right_option();
        let mut state = State::default();
        state.reduce(Input::KeyDown(Key::LeftShift), &config); // key-up never arrives
        assert!(
            state
                .reduce(Input::KeyDown(Key::RightOption), &config)
                .events
                .is_empty(),
            "the phantom Shift blocks the binding"
        );
        state.reduce(Input::KeyUp(Key::RightOption), &config);

        state.reduce(Input::Reset, &config);
        assert_eq!(
            state
                .reduce(Input::KeyDown(Key::RightOption), &config)
                .events,
            vec![Event::Activated(Role::Hold)],
            "a resync must bring the dictate key back"
        );
    }
}
