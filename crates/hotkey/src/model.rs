//! Platform-neutral binding model.
//!
//! The Swift app keyed everything off macOS virtual keycodes (55 = Command,
//! 63 = Fn, ...). Those mean nothing on Windows, so bindings are expressed as
//! a `Key` enum here and each backend maps its native codes into it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A physical key, side-specific where the distinction matters (the Swift app
/// let you bind Right Option specifically, which is a popular dictation key).
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
    pub const fn is_modifier(self) -> bool {
        matches!(
            self,
            Key::LeftCommand
                | Key::RightCommand
                | Key::LeftShift
                | Key::RightShift
                | Key::LeftOption
                | Key::RightOption
                | Key::LeftControl
                | Key::RightControl
                | Key::Fn
        )
    }

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

/// Set of side-agnostic modifiers, mirroring Swift's `ShortcutModifiers`.
pub type Modifiers = BTreeSet<Modifier>;

/// What triggers a binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Binding {
    Disabled,
    /// A regular key, optionally with modifiers held (e.g. Cmd+Shift+D).
    Key {
        key: Key,
        modifiers: Modifiers,
    },
    /// A modifier held on its own (e.g. hold Right Option) — the case Tauri's
    /// global-shortcut plugin cannot express, and why this crate exists.
    ModifierKey {
        key: Key,
        modifiers: Modifiers,
    },
}

impl Binding {
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

/// What a binding does. Ported from Swift's `ShortcutRole`.
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
    Command,
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
        Role::Command,
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
    pub command: Option<Binding>,
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
            Role::Command => &self.command,
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

/// Default bindings. Hold-to-talk on Right Option: a modifier-only hold that
/// exists on both platforms and rarely collides with app shortcuts.
impl Config {
    pub fn defaults() -> Self {
        Self {
            hold: Some(Binding::ModifierKey {
                key: Key::RightOption,
                modifiers: Modifiers::new(),
            }),
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
