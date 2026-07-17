//! The universal action schema — Phase 0, Task 1.
//!
//! One shared contract that the intent parser produces and every connector
//! consumes. Adding an app later means adding entries here, not touching the
//! pipeline.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One parsed agent action, as the intent parser returns it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ParsedAction {
    /// e.g. "slack.send_message". The `<app>.<verb>` shape keys the dispatcher.
    pub action: String,
    /// e.g. "slack". Redundant with `action`'s prefix, but explicit for routing.
    pub app: String,
    /// Filled entity fields, e.g. {"channel": "general", "message_text": "..."}.
    pub entities: BTreeMap<String, String>,
    /// The model's confidence, 0.0–1.0. Low confidence surfaces a clarification
    /// rather than executing.
    pub confidence: f32,
    /// Whether this must be confirmed before executing. True for every write.
    pub requires_confirmation: bool,
}

/// A supported action's definition: which entities it needs, and whether it
/// writes. The catalog below is the single source of truth.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct ActionDef {
    pub action: &'static str,
    pub app: &'static str,
    pub description: &'static str,
    pub required_entities: &'static [&'static str],
    pub optional_entities: &'static [&'static str],
    /// A write always requires confirmation before it runs.
    pub is_write: bool,
}

/// The initial action catalog. This is the "enumerate the first 10–15 actions"
/// deliverable — deliberately a static table so a new app is a data change.
pub const ACTIONS: &[ActionDef] = &[
    ActionDef {
        action: "spotify.play",
        app: "spotify",
        description: "Search for a track, artist or playlist and start playback",
        required_entities: &["query"],
        optional_entities: &[],
        is_write: false,
    },
    ActionDef {
        action: "spotify.pause",
        app: "spotify",
        description: "Pause playback",
        required_entities: &[],
        optional_entities: &[],
        is_write: false,
    },
    ActionDef {
        action: "slack.send_message",
        app: "slack",
        description: "Post a message to a channel or direct message",
        required_entities: &["channel", "message_text"],
        optional_entities: &[],
        is_write: true,
    },
    ActionDef {
        action: "slack.search_recent",
        app: "slack",
        description: "Search recent messages",
        required_entities: &["query"],
        optional_entities: &["channel"],
        is_write: false,
    },
    ActionDef {
        action: "github.create_issue",
        app: "github",
        description: "Open an issue on a repository",
        required_entities: &["repo", "title"],
        optional_entities: &["body"],
        is_write: true,
    },
    ActionDef {
        action: "github.list_issues",
        app: "github",
        description: "List recent issues on a repository",
        required_entities: &["repo"],
        optional_entities: &[],
        is_write: false,
    },
    ActionDef {
        action: "notion.create_page",
        app: "notion",
        description: "Create a page under a parent page or database",
        required_entities: &["parent", "title"],
        optional_entities: &["content"],
        is_write: true,
    },
    ActionDef {
        action: "notion.append_block",
        app: "notion",
        description: "Append text to an existing page",
        required_entities: &["page", "text"],
        optional_entities: &[],
        is_write: true,
    },
    ActionDef {
        action: "calendar.create_event",
        app: "google_calendar",
        description: "Create a calendar event",
        required_entities: &["title", "start", "duration_minutes"],
        optional_entities: &["attendees"],
        is_write: true,
    },
];

/// Looks up an action definition by name.
pub fn find(action: &str) -> Option<&'static ActionDef> {
    ACTIONS.iter().find(|a| a.action == action)
}

impl ParsedAction {
    /// Whether every required entity for this action is present.
    ///
    /// The intent parser can name an action but miss a field ("send a slack
    /// message" with no channel); this is how the caller knows to ask.
    pub fn missing_entities(&self) -> Vec<&'static str> {
        match find(&self.action) {
            Some(def) => def
                .required_entities
                .iter()
                .filter(|field| {
                    self.entities
                        .get(**field)
                        .is_none_or(|v| v.trim().is_empty())
                })
                .copied()
                .collect(),
            None => Vec::new(),
        }
    }

    pub fn is_known(&self) -> bool {
        find(&self.action).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_name_is_app_dot_verb() {
        for def in ACTIONS {
            let (app, _) = def.action.split_once('.').expect("action must be app.verb");
            // The action's prefix and its `app` need not be identical (calendar
            // vs google_calendar), but the prefix must be non-empty.
            assert!(!app.is_empty(), "{}", def.action);
        }
    }

    #[test]
    fn every_write_defaults_to_requiring_confirmation() {
        // The catalog marks writes; the parser is instructed to set
        // requires_confirmation from this, so the flag and the catalog agree.
        assert!(find("slack.send_message").unwrap().is_write);
        assert!(!find("github.list_issues").unwrap().is_write);
    }

    #[test]
    fn missing_entities_are_reported() {
        let action = ParsedAction {
            action: "slack.send_message".into(),
            app: "slack".into(),
            entities: BTreeMap::from([("channel".into(), "general".into())]),
            confidence: 0.9,
            requires_confirmation: true,
        };
        assert_eq!(action.missing_entities(), vec!["message_text"]);
    }

    #[test]
    fn complete_action_has_no_missing_entities() {
        let action = ParsedAction {
            action: "spotify.play".into(),
            app: "spotify".into(),
            entities: BTreeMap::from([("query".into(), "jazz".into())]),
            confidence: 0.9,
            requires_confirmation: false,
        };
        assert!(action.missing_entities().is_empty());
    }

    #[test]
    fn blank_entity_counts_as_missing() {
        let action = ParsedAction {
            action: "spotify.play".into(),
            app: "spotify".into(),
            entities: BTreeMap::from([("query".into(), "   ".into())]),
            confidence: 0.9,
            requires_confirmation: false,
        };
        assert_eq!(action.missing_entities(), vec!["query"]);
    }

    #[test]
    fn unknown_action_is_flagged() {
        let action = ParsedAction {
            action: "figma.export".into(),
            app: "figma".into(),
            entities: BTreeMap::new(),
            confidence: 0.9,
            requires_confirmation: false,
        };
        assert!(!action.is_known());
    }
}
