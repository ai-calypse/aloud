//! Turning a raw accessibility traversal into something worth sending a model.
//!
//! The raw tree is the wrong shape for a prompt in three ways, each measured on
//! this machine against a real app (Finder, 269 elements, ~26KB / ~8k tokens):
//!
//! - **Most of it cannot be clicked.** 235 of those 269 were `AXMenuItem`s
//!   inside closed menus. They are real elements, but clicking one at its
//!   reported coordinates does nothing, because the menu is not open. Sending
//!   them teaches the model to try.
//! - **Unlabelled elements are unusable.** An `AXGroup` with no text is
//!   something the model can only click blindly.
//! - **Role names are doubled** — `"AXButton (button)"` — and the `AX` prefix is
//!   noise the model does not need.
//!
//! Filtering to labelled, actionable, on-screen elements takes a real app to
//! ~15-30 entries. That is the difference between a traversal per turn being
//! affordable and not.

use serde_json::{json, Value};

/// Elements past this are dropped. A screen with more actionable controls than
/// this is a screen where the model should be narrowing down by other means,
/// and an unbounded list is an unbounded prompt.
const MAX_ELEMENTS: usize = 60;

/// Roles that respond to a click at their coordinates. Deliberately a list of
/// what works rather than a list of what to skip: the AX role vocabulary is
/// open-ended, and an unknown role is far more likely to be scenery than a
/// control. `AXMenuItem` is absent on purpose — see the module comment.
const CLICKABLE: &[&str] = &[
    "AXButton",
    "AXCheckBox",
    "AXRadioButton",
    "AXPopUpButton",
    "AXMenuButton",
    "AXMenuBarItem",
    "AXTextField",
    "AXTextArea",
    "AXSearchField",
    "AXComboBox",
    "AXLink",
    "AXTab",
    "AXSlider",
    "AXDisclosureTriangle",
    "AXCell",
    "AXRow",
    "AXImage",
    "AXStaticText",
];

/// Reduces a raw traversal to the elements worth showing, as
/// `{ app, elements: [{ role, text, x, y }] }`.
///
/// The coordinates are the element's centre, not its origin: the origin of a
/// wide control is its left edge, and for a table row that lands on padding
/// rather than the row. Clicking the middle is what a person does.
pub fn summarise(raw: &Value) -> Value {
    let app = raw["app_name"].as_str().unwrap_or("the focused app");
    let mut seen = std::collections::HashSet::new();

    let elements: Vec<Value> = raw["elements"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            let role = role_of(e)?;
            let text = e["text"]
                .as_str()
                .map(str::trim)
                .filter(|t| !t.is_empty())?;
            let (w, h) = (num(&e["width"])?, num(&e["height"])?);
            // A zero-area element is present in the tree but not on the screen.
            if w <= 0 || h <= 0 {
                return None;
            }
            let (x, y) = (num(&e["x"])? + w / 2, num(&e["y"])? + h / 2);
            // Off-screen elements traverse fine and click nowhere useful.
            if x < 0 || y < 0 {
                return None;
            }
            // The same control routinely appears twice — once as `AXCell` and
            // once as the `AXStaticText` inside it — at the same point.
            if !seen.insert((x, y, text.to_string())) {
                return None;
            }
            Some(json!({ "role": role, "text": text, "x": x, "y": y }))
        })
        .take(MAX_ELEMENTS)
        .collect();

    json!({
        "app": app,
        "elements": elements,
        "hint": "x and y are click targets for the click tool. Re-read after \
                 anything that changes the screen.",
    })
}

/// The element's role, shortened, or `None` when it is not something a click
/// acts on. The helper reports `"AXButton (button)"`; only the first word is
/// meaningful, and the `AX` prefix is noise.
fn role_of(element: &Value) -> Option<&'static str> {
    let raw = element["role"].as_str()?;
    let head = raw.split_whitespace().next()?;
    CLICKABLE.iter().find(|c| **c == head).map(|c| &c[2..]) // strip "AX"
}

/// The helper emits coordinates as JSON numbers, but they arrive as either
/// integers or floats depending on the element.
fn num(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(role: &str, text: &str, x: i64, y: i64, w: i64, h: i64) -> Value {
        json!({ "role": role, "text": text, "x": x, "y": y, "width": w, "height": h })
    }

    fn summarise_of(elements: Vec<Value>) -> Value {
        summarise(&json!({ "app_name": "Test", "elements": elements }))
    }

    fn texts(v: &Value) -> Vec<String> {
        v["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["text"].as_str().unwrap().to_string())
            .collect()
    }

    /// The measured failure this whole module exists for: a real Finder
    /// traversal was 235 closed-menu items out of 269 elements. Clicking one
    /// does nothing, because its menu is not open.
    #[test]
    fn closed_menu_items_are_dropped_but_the_menu_bar_is_kept() {
        let out = summarise_of(vec![
            element("AXMenuItem (menu item)", "New Window", 10, 40, 100, 20),
            element("AXMenuBarItem (menu bar item)", "File", 106, 0, 42, 33),
        ]);
        assert_eq!(texts(&out), ["File"]);
    }

    #[test]
    fn unlabelled_and_offscreen_elements_are_dropped() {
        let out = summarise_of(vec![
            element("AXButton (button)", "", 10, 10, 50, 20),
            element("AXButton (button)", "   ", 10, 10, 50, 20),
            element("AXButton (button)", "Offscreen", -400, 10, 50, 20),
            element("AXButton (button)", "Zero area", 10, 10, 0, 20),
            element("AXButton (button)", "Send", 10, 10, 50, 20),
        ]);
        assert_eq!(texts(&out), ["Send"]);
    }

    /// Coordinates must be the centre. A row reported at its origin clicks the
    /// left padding; a wide button clicks its edge.
    #[test]
    fn coordinates_are_the_centre_of_the_element() {
        let out = summarise_of(vec![element("AXButton (button)", "Send", 100, 200, 80, 40)]);
        let e = &out["elements"][0];
        assert_eq!((e["x"].as_i64(), e["y"].as_i64()), (Some(140), Some(220)));
    }

    /// A cell and the static text inside it are the same control at the same
    /// point; sending both gives the model two ways to say one thing.
    #[test]
    fn the_same_control_reported_twice_appears_once() {
        let out = summarise_of(vec![
            element("AXCell (cell)", "Report.pdf", 10, 10, 200, 20),
            element("AXStaticText (text)", "Report.pdf", 10, 10, 200, 20),
        ]);
        assert_eq!(texts(&out), ["Report.pdf"]);
    }

    #[test]
    fn roles_are_shortened_and_unknown_ones_dropped() {
        let out = summarise_of(vec![
            element("AXButton (button)", "Send", 10, 10, 50, 20),
            element("AXGroup (group)", "Scenery", 10, 60, 50, 20),
        ]);
        assert_eq!(texts(&out), ["Send"]);
        assert_eq!(out["elements"][0]["role"], "Button");
    }

    /// An unbounded element list is an unbounded prompt.
    #[test]
    fn the_list_is_capped() {
        let many = (0..500)
            .map(|i| element("AXButton (button)", &format!("b{i}"), 0, i, 10, 10))
            .collect();
        assert_eq!(
            summarise_of(many)["elements"].as_array().unwrap().len(),
            MAX_ELEMENTS
        );
    }

    #[test]
    fn a_traversal_with_nothing_usable_summarises_to_an_empty_list() {
        let out = summarise(&json!({ "app_name": "Empty" }));
        assert_eq!(out["elements"].as_array().unwrap().len(), 0);
        assert_eq!(out["app"], "Empty");
    }
}
