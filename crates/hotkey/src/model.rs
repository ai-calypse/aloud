//! Platform-neutral binding model: bindings are a `Key` enum, and each backend
//! maps its native keycodes into it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A physical key, side-specific where the distinction matters — Right Option
/// alone is a popular dictation binding.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum Key {
    LeftCommand,
    RightCommand,
    LeftShift,
    RightShift,
    LeftOption,
    RightOption,
    LeftControl,
    RightControl,
    /// macOS only; no Windows equivalent.
    Fn,
    Escape,
    Space,
    Char(char),
    F(u8),
}

impl Key {
    /// The side-agnostic modifier this key contributes, if any.
    pub const fn logical_modifier(self) -> Option<Modifier> {
        match self {
            Key::LeftCommand | Key::RightCommand => Some(Modifier::Command),
            Key::LeftShift | Key::RightShift => Some(Modifier::Shift),
            Key::LeftOption | Key::RightOption => Some(Modifier::Option),
            Key::LeftControl | Key::RightControl => Some(Modifier::Control),
            Key::Fn => Some(Modifier::Function),
            _ => None,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum Modifier {
    Command,
    Control,
    Option,
    Shift,
    Function,
}

/// Set of side-agnostic modifiers.
pub type Modifiers = BTreeSet<Modifier>;

/// What triggers a binding.
///
/// `modifiers` is side-agnostic (`Option` matches either Alt key). Only a
/// *recording* fills `exact_modifiers`, which pins the chord to those exact
/// keys so Left-Option+Z is not Right-Option+Z; empty means either side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Binding {
    Disabled,
    Key {
        key: Key,
        modifiers: Modifiers,
        #[serde(default)]
        exact_modifiers: BTreeSet<Key>,
    },
    /// A modifier held on its own — the case Tauri's global-shortcut plugin
    /// cannot express, and why this crate exists.
    ModifierKey {
        key: Key,
        modifiers: Modifiers,
        #[serde(default)]
        exact_modifiers: BTreeSet<Key>,
    },
}

impl Binding {
    /// A plain-key binding with side-agnostic modifiers. Recordings build the
    /// side-specific form via the struct literal instead.
    pub fn key_chord(key: Key, modifiers: Modifiers) -> Self {
        Binding::Key {
            key,
            modifiers,
            exact_modifiers: BTreeSet::new(),
        }
    }

    /// A modifier-held binding with side-agnostic modifiers.
    pub fn modifier_chord(key: Key, modifiers: Modifiers) -> Self {
        Binding::ModifierKey {
            key,
            modifiers,
            exact_modifiers: BTreeSet::new(),
        }
    }

    pub fn is_disabled(&self) -> bool {
        matches!(self, Binding::Disabled)
    }

    pub fn modifiers(&self) -> Option<&Modifiers> {
        match self {
            Binding::Disabled => None,
            Binding::Key { modifiers, .. } | Binding::ModifierKey { modifiers, .. } => {
                Some(modifiers)
            }
        }
    }

    /// The side-specific modifier keys this binding pins to, if any. Empty (or
    /// `None` for `Disabled`) means either side satisfies each modifier.
    pub fn exact_modifiers(&self) -> Option<&BTreeSet<Key>> {
        match self {
            Binding::Disabled => None,
            Binding::Key {
                exact_modifiers, ..
            }
            | Binding::ModifierKey {
                exact_modifiers, ..
            } => Some(exact_modifiers),
        }
    }

    pub fn key(&self) -> Option<Key> {
        match self {
            Binding::Disabled => None,
            Binding::Key { key, .. } | Binding::ModifierKey { key, .. } => Some(*key),
        }
    }

    /// More modifiers = more specific. Used to order simultaneous activations
    /// so Cmd+Opt+D wins over a bare Cmd binding.
    pub fn specificity(&self) -> usize {
        self.modifiers().map_or(0, BTreeSet::len)
    }
}

/// What a binding does.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// Record while held.
    Hold,
    /// Tap to start, tap again to stop.
    Toggle,
    /// Re-paste the last transcript.
    CopyAgain,
    /// Record while held, then answer instead of pasting.
    Ask,
    /// Record while held, then transform the current selection with it.
    Rewrite,
    /// Start/stop a meeting-notes session.
    MeetingNotes,
    /// Record while held, then execute the spoken command across apps.
    Agent,
}

impl Role {
    pub const ALL: [Role; 7] = [
        Role::Hold,
        Role::Toggle,
        Role::CopyAgain,
        Role::Ask,
        Role::Rewrite,
        Role::MeetingNotes,
        Role::Agent,
    ];

    /// One-shots fire on the leading edge and emit no deactivation.
    pub const fn is_one_shot(self) -> bool {
        matches!(self, Role::CopyAgain | Role::MeetingNotes | Role::Toggle)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Config {
    pub hold: Option<Binding>,
    pub toggle: Option<Binding>,
    pub copy_again: Option<Binding>,
    pub ask: Option<Binding>,
    /// Called `command` in older settings files; `Settings::load` migrates them
    /// rather than a serde alias, which makes specta emit a union whose
    /// `keyof Config` is `never` and breaks the settings UI.
    pub rewrite: Option<Binding>,
    pub meeting_notes: Option<Binding>,
    pub agent: Option<Binding>,
}

impl Config {
    pub fn binding(&self, role: Role) -> &Binding {
        let slot = match role {
            Role::Hold => &self.hold,
            Role::Toggle => &self.toggle,
            Role::CopyAgain => &self.copy_again,
            Role::Ask => &self.ask,
            Role::Rewrite => &self.rewrite,
            Role::MeetingNotes => &self.meeting_notes,
            Role::Agent => &self.agent,
        };
        slot.as_ref().unwrap_or(&Binding::Disabled)
    }

    pub fn roles(&self) -> impl Iterator<Item = (Role, &Binding)> {
        Role::ALL
            .into_iter()
            .map(|role| (role, self.binding(role)))
            .filter(|(_, b)| !b.is_disabled())
    }
}

/// Hold-to-talk on Right Option: exists on both platforms and rarely collides
/// with app shortcuts.
impl Config {
    pub fn defaults() -> Self {
        Self {
            hold: Some(Binding::modifier_chord(Key::RightOption, Modifiers::new())),
            ..Default::default()
        }
    }
}

/// Emitted when a binding activates or deactivates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    Activated(Role),
    Deactivated(Role),
    /// Escape pressed while a session is in flight.
    Cancelled,
}
