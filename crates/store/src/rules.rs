//! App rules: vary dictation behaviour by the app that has focus. Two knobs
//! only — whether cleanup runs and which prompt it uses; everything else is
//! global.
//!
//! Name matching is case-insensitive substring ("code" matches "Visual Studio
//! Code") and the first match wins. A rule carrying a bundle id prefers that,
//! which survives the app being renamed or localised.

use serde::{Deserialize, Serialize};

/// One per-app override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AppRule {
    /// Substring of the app's name to match. Empty never matches.
    pub app_matches: String,
    /// The app's `CFBundleIdentifier`, when the rule was written by picking an
    /// app. Additive: older rules deserialise with None and keep matching on
    /// [`app_matches`], so there is no migration.
    ///
    /// [`app_matches`]: AppRule::app_matches
    #[serde(default)]
    pub bundle_id: Option<String>,
    /// Override cleanup on/off for this app. None leaves the global setting.
    pub post_processing_enabled: Option<bool>,
    /// A prompt to use instead of the global one. Empty leaves the global.
    pub custom_prompt: String,
}

impl AppRule {
    fn wants_bundle(&self) -> Option<&str> {
        self.bundle_id
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
    }

    fn matches(&self, app: Option<&str>, bundle_id: Option<&str>) -> bool {
        // Exact, never a substring: "com.apple.mail" must not match
        // "com.apple.mailer".
        if let (Some(want), Some(got)) = (self.wants_bundle(), bundle_id) {
            return got.eq_ignore_ascii_case(want);
        }
        // Fall through to the name when the focused app reports no bundle id —
        // Windows never does, nor does a plain binary.
        let needle = self.app_matches.trim().to_lowercase();
        !needle.is_empty() && app.is_some_and(|a| a.to_lowercase().contains(&needle))
    }
}

/// The first rule matching the focused app, if any. None when no app is focused
/// or no rule matches — the caller then uses the global settings unchanged.
pub fn matching<'a>(
    rules: &'a [AppRule],
    app: Option<&str>,
    bundle_id: Option<&str>,
) -> Option<&'a AppRule> {
    if app.is_none() && bundle_id.is_none() {
        return None;
    }
    rules.iter().find(|r| r.matches(app, bundle_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(app_matches: &str) -> AppRule {
        AppRule {
            app_matches: app_matches.into(),
            bundle_id: None,
            post_processing_enabled: None,
            custom_prompt: String::new(),
        }
    }

    fn keyed(app_matches: &str, bundle_id: &str) -> AppRule {
        AppRule {
            bundle_id: Some(bundle_id.into()),
            ..rule(app_matches)
        }
    }

    #[test]
    fn substring_match_is_case_insensitive() {
        let rules = [rule("code")];
        assert!(matching(&rules, Some("Visual Studio Code"), None).is_some());
        assert!(matching(&rules, Some("VSCODE"), None).is_some());
    }

    #[test]
    fn first_matching_rule_wins() {
        let rules = [rule("slack"), rule("s")];
        let hit = matching(&rules, Some("Slack"), None).unwrap();
        assert_eq!(hit.app_matches, "slack");
    }

    #[test]
    fn no_focused_app_matches_nothing() {
        let rules = [rule("slack")];
        assert!(matching(&rules, None, None).is_none());
    }

    #[test]
    fn empty_matcher_never_fires() {
        let rules = [rule("   ")];
        assert!(matching(&rules, Some("anything"), None).is_none());
    }

    #[test]
    fn non_matching_app_returns_none() {
        let rules = [rule("slack")];
        assert!(matching(&rules, Some("Terminal"), None).is_none());
    }

    #[test]
    fn a_bundle_keyed_rule_survives_the_app_being_renamed() {
        let rules = [keyed("Slack", "com.tinyspeck.slackmacgap")];
        assert!(matching(
            &rules,
            Some("Slack Beta"),
            Some("com.tinyspeck.slackmacgap")
        )
        .is_some());
        assert!(matching(
            &rules,
            Some("Totally Different"),
            Some("com.tinyspeck.slackmacgap")
        )
        .is_some());
    }

    /// Reusing the name matcher would fire "com.apple.mail" inside
    /// "com.apple.mailer".
    #[test]
    fn bundle_ids_match_whole_not_as_substrings() {
        let rules = [keyed("Mail", "com.apple.mail")];
        assert!(matching(&rules, None, Some("com.apple.mail")).is_some());
        assert!(matching(&rules, None, Some("com.apple.mailer")).is_none());
        assert!(matching(&rules, None, Some("COM.APPLE.MAIL")).is_some());
    }

    /// Guards: bundle ids are macOS-only, so a keyed rule that refused to fall
    /// back on the name would stop working on Windows.
    #[test]
    fn a_bundle_keyed_rule_still_fires_where_no_bundle_id_is_reported() {
        let rules = [keyed("slack", "com.tinyspeck.slackmacgap")];
        assert!(matching(&rules, Some("Slack"), None).is_some());
    }

    /// Rules written before the field existed must keep matching on the name.
    #[test]
    fn a_legacy_rule_deserialises_without_a_bundle_id_and_still_matches() {
        let json = r#"{"app_matches":"slack","post_processing_enabled":false,"custom_prompt":""}"#;
        let legacy: AppRule = serde_json::from_str(json).unwrap();
        assert_eq!(legacy.bundle_id, None);
        assert!(matching(
            std::slice::from_ref(&legacy),
            Some("Slack"),
            Some("com.anything")
        )
        .is_some());
    }
}
