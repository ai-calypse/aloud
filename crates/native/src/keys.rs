//! Keyboard shortcuts as the model writes them ("cmd+shift+5", "⌘⇧5",
//! "control command q"), turned into a chord `aloud_paste::press_chord` presses.
//!
//! One tool for every shortcut instead of one tool per action: anything the
//! Mac or an app does from the keyboard — the screenshot and recording
//! toolbar, locking the screen, a new tab — is reachable by its keys.

use aloud_paste::{ChordKey, Modifier};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Chord {
    pub modifiers: Vec<Modifier>,
    pub key: ChordKey,
}

pub(crate) fn parse(spec: &str) -> Result<Chord, String> {
    let spaced: String = spec
        .chars()
        .flat_map(|c| match c {
            '⌘' => " cmd ".chars().collect::<Vec<_>>(),
            '⇧' => " shift ".chars().collect(),
            '⌥' => " option ".chars().collect(),
            '⌃' => " control ".chars().collect(),
            '+' => vec![' '],
            c => vec![c],
        })
        .collect();
    let mut modifiers = Vec::new();
    let mut key = None;
    for word in spaced.split_whitespace() {
        let modifier = match word.to_lowercase().as_str() {
            "cmd" | "command" | "meta" | "super" => Some(Modifier::Command),
            "shift" => Some(Modifier::Shift),
            "opt" | "option" | "alt" => Some(Modifier::Option),
            "ctrl" | "control" | "ctl" => Some(Modifier::Control),
            // Apps cannot press fn/globe; the chords that need it have menu or
            // app equivalents.
            "fn" | "globe" => return Err("the fn/globe key can't be pressed by apps".into()),
            _ => None,
        };
        match modifier {
            Some(m) if !modifiers.contains(&m) => modifiers.push(m),
            Some(_) => {}
            None if key.is_none() => key = Some(key_of(word)?),
            None => {
                return Err(format!(
                    "{spec:?} names two keys; press one chord at a time"
                ))
            }
        }
    }
    let key = key.ok_or_else(|| format!("{spec:?} has modifiers but no key"))?;
    Ok(Chord { modifiers, key })
}

fn key_of(word: &str) -> Result<ChordKey, String> {
    // macOS virtual key codes (Events.h, kVK_*).
    let code = match word.to_lowercase().as_str() {
        "return" | "enter" => 36,
        "tab" => 48,
        "space" | "spacebar" => 49,
        "delete" | "backspace" => 51,
        "forwarddelete" | "del" => 117,
        "escape" | "esc" => 53,
        "left" => 123,
        "right" => 124,
        "down" => 125,
        "up" => 126,
        "home" => 115,
        "end" => 119,
        "pageup" => 116,
        "pagedown" => 121,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        _ => {
            let mut chars = word.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) => Ok(ChordKey::Char(c.to_ascii_lowercase())),
                _ => Err(format!("unknown key {word:?}")),
            };
        }
    };
    Ok(ChordKey::Code(code))
}

/// Chords that close, quit, delete or sign out — confirmed first. Everything
/// else a voice command would press is undone by pressing something else.
pub(crate) fn destructive(chord: &Chord) -> bool {
    let held = |m| chord.modifiers.contains(&m);
    let (cmd, shift, option, control) = (
        held(Modifier::Command),
        held(Modifier::Shift),
        held(Modifier::Option),
        held(Modifier::Control),
    );
    match chord.key {
        // Quit, close, and ⌘⇧Q log out; ⌘⌥Esc force quit. ⌃⌘Q is lock screen,
        // which loses nothing.
        ChordKey::Char('q' | 'w') if cmd && !control => true,
        ChordKey::Code(53) if cmd && option => true,
        // ⌘⌫ moves to the Trash in Finder; ⌘⇧⌫ empties it.
        ChordKey::Code(51) if cmd => true,
        ChordKey::Code(117) if cmd || shift => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_and_symbol_chords_parse_the_same() {
        let toolbar = Chord {
            modifiers: vec![Modifier::Command, Modifier::Shift],
            key: ChordKey::Char('5'),
        };
        assert_eq!(parse("cmd+shift+5").unwrap(), toolbar);
        assert_eq!(parse("⌘⇧5").unwrap(), toolbar);
        assert_eq!(parse("Command Shift 5").unwrap(), toolbar);
        assert_eq!(
            parse("ctrl+cmd+q").unwrap(),
            Chord {
                modifiers: vec![Modifier::Control, Modifier::Command],
                key: ChordKey::Char('q')
            }
        );
        assert_eq!(parse("cmd+space").unwrap().key, ChordKey::Code(49));
        assert_eq!(
            parse("F11").unwrap(),
            Chord {
                modifiers: vec![],
                key: ChordKey::Code(103)
            }
        );
    }

    #[test]
    fn nonsense_chords_are_refused() {
        assert!(parse("cmd+shift").is_err());
        assert!(parse("cmd+a+b").is_err());
        assert!(parse("fn+f").is_err());
        assert!(parse("cmd+banana").is_err());
    }

    #[test]
    fn quitting_closing_and_deleting_are_destructive() {
        for spec in [
            "cmd+q",
            "cmd+w",
            "cmd+shift+q",
            "cmd+option+esc",
            "cmd+delete",
        ] {
            assert!(destructive(&parse(spec).unwrap()), "{spec}");
        }
        for spec in ["cmd+shift+5", "ctrl+cmd+q", "cmd+t", "cmd+space", "escape"] {
            assert!(!destructive(&parse(spec).unwrap()), "{spec}");
        }
    }
}
