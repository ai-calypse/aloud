//! App rules: vary dictation behaviour by the app that has focus.
//!
//! "In Slack, keep it casual — don't punctuate." "In my editor, paste raw."
//! A rule matches the focused app's name and overrides a couple of settings
//! for that dictation only. Kept deliberately small: the two knobs that change
//! per-app are whether cleanup runs and which prompt it uses. Everything else
//! (engine, model, keys) is global.
//!
//! Matching is case-insensitive substring, so "code" matches "Visual Studio
//! Code", and the first matching rule wins.

use serde::{Deserialize, Serialize};

/// One per-app override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AppRule {
    /// Substring of the app's name to match, e.g. "slack", "code". Empty never
    /// matches.
    pub app_matches: String,
    /// Override cleanup on/off for this app. None leaves the global setting.
    pub post_processing_enabled: Option<bool>,
    /// A prompt to use instead of the global one. Empty leaves the global.
    pub custom_prompt: String,
}

impl AppRule {
    fn matches(&self, app: &str) -> bool {
        let needle = self.app_matches.trim().to_lowercase();
        !needle.is_empty() && app.to_lowercase().contains(&needle)
    }
}

/// The first rule matching `app`, if any. None when no app is focused or no
/// rule matches — the caller then uses the global settings unchanged.
pub fn matching<'a>(rules: &'a [AppRule], app: Option<&str>) -> Option<&'a AppRule> {
    let app = app?;
    rules.iter().find(|r| r.matches(app))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(app_matches: &str) -> AppRule {
        AppRule {
            app_matches: app_matches.into(),
            post_processing_enabled: None,
            custom_prompt: String::new(),
        }
    }

    #[test]
    fn substring_match_is_case_insensitive() {
        let rules = [rule("code")];
        assert!(matching(&rules, Some("Visual Studio Code")).is_some());
        assert!(matching(&rules, Some("VSCODE")).is_some());
    }

    #[test]
    fn first_matching_rule_wins() {
        let rules = [rule("slack"), rule("s")];
        // Both match "Slack"; the first is returned.
        let hit = matching(&rules, Some("Slack")).unwrap();
        assert_eq!(hit.app_matches, "slack");
    }

    #[test]
    fn no_focused_app_matches_nothing() {
        let rules = [rule("slack")];
        assert!(matching(&rules, None).is_none());
    }

    #[test]
    fn empty_matcher_never_fires() {
        let rules = [rule("   ")];
        assert!(matching(&rules, Some("anything")).is_none());
    }

    #[test]
    fn non_matching_app_returns_none() {
        let rules = [rule("slack")];
        assert!(matching(&rules, Some("Terminal")).is_none());
    }
}
