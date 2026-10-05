//! Whether a macOS Focus (Do Not Disturb, Work, Sleep…) is on, so the notch
//! can hold ordinary system events while it is.
//!
//! macOS has no public call for this that works without a permission prompt;
//! `INFocusStatusCenter` needs one and says only on/off. The assertion file
//! says the same without the prompt, but it sits behind Full Disk Access and
//! its layout has moved between releases — so every failure reads as "off":
//! a missing grant must never mute the notch.
//!
//! ponytail: reads `Assertions.json`. `INFocusStatusCenter` is the supported
//! upgrade once the app asks for Focus Status.

/// `~/Library/DoNotDisturb/DB/Assertions.json`: a Focus is on while the first
/// store has any assertion records.
pub fn parse_assertions(text: &str) -> bool {
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    doc.get("data")
        .and_then(|d| d.as_array())
        .into_iter()
        .flatten()
        .filter_map(|store| store.get("storeAssertionRecords")?.as_array())
        .any(|records| !records.is_empty())
}

#[cfg(target_os = "macos")]
pub fn is_on() -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    let path = std::path::Path::new(&home).join("Library/DoNotDisturb/DB/Assertions.json");
    std::fs::read_to_string(path).is_ok_and(|text| parse_assertions(&text))
}

#[cfg(not(target_os = "macos"))]
pub fn is_on() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_focus_is_on_while_the_store_holds_an_assertion() {
        let on = r#"{"data":[{"storeAssertionRecords":[{"assertionDetails":{}}]}]}"#;
        assert!(parse_assertions(on));
        let off = r#"{"data":[{"storeAssertionRecords":[]}]}"#;
        assert!(!parse_assertions(off));
    }

    /// The failure: an unreadable or reshaped file muting every event.
    #[test]
    fn anything_unreadable_reads_as_off() {
        for text in ["", "not json", "{}", r#"{"data":[{}]}"#, r#"{"data":3}"#] {
            assert!(!parse_assertions(text), "{text:?}");
        }
    }
}
