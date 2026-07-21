//! App rules: vary dictation behaviour by the app that has focus.
//!
//! "In Slack, keep it casual — don't punctuate." "In my editor, paste raw."
//! A rule matches the focused app's name and overrides a couple of settings
//! for that dictation only. Kept deliberately small: the two knobs that change
//! per-app are whether cleanup runs and which prompt it uses. Everything else
//! (engine, model, keys) is global.
//!
//! Name matching is case-insensitive substring, so "code" matches "Visual
//! Studio Code", and the first matching rule wins. A rule that carries a bundle
//! id prefers that: `com.tinyspeck.slackmacgap` survives Slack being renamed,
//! localised, or installed somewhere else.

use serde::{Deserialize, Serialize};

/// One per-app override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AppRule {
    /// Substring of the app's name to match, e.g. "slack", "code". Empty never
    /// matches.
    pub app_matches: String,
    /// The app's `CFBundleIdentifier`, when the rule was written by picking an
    /// app rather than typing a name.
    ///
    /// Additive on purpose: every rule written before this field existed
    /// deserialises with None and keeps matching on [`app_matches`], so there
    /// is no migration to run and nothing to get wrong. Rules acquire a bundle
    /// id lazily, the next time the user edits one.
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
    /// The rule's bundle id, if it has a usable one.
    fn wants_bundle(&self) -> Option<&str> {
        self.bundle_id
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
    }

    fn matches(&self, app: Option<&str>, bundle_id: Option<&str>) -> bool {
        // A bundle id is an exact identifier, never a substring: "com.apple.mail"
        // must not match "com.apple.mailer".
        if let (Some(want), Some(got)) = (self.wants_bundle(), bundle_id) {
            return got.eq_ignore_ascii_case(want);
        }
        // Falling through to the name when the *focused* app has no bundle id
        // is deliberate: Windows reports none at all, and a plain binary has
        // none either. Without this a bundle-keyed rule would silently stop
        // firing everywhere the OS cannot answer.
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
        // Both match "Slack"; the first is returned.
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

    /// The point of keying on a bundle id: the app's display name changed and
    /// the rule must still fire.
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

    /// A bundle id is an identifier, not a substring. Reusing the name matcher
    /// here would fire "com.apple.mail" inside "com.apple.mailer".
    #[test]
    fn bundle_ids_match_whole_not_as_substrings() {
        let rules = [keyed("Mail", "com.apple.mail")];
        assert!(matching(&rules, None, Some("com.apple.mail")).is_some());
        assert!(matching(&rules, None, Some("com.apple.mailer")).is_none());
        // Case is not significant; reverse-DNS ids are conventionally lower.
        assert!(matching(&rules, None, Some("COM.APPLE.MAIL")).is_some());
    }

    /// The regression this guards: bundle ids are macOS-only. If a keyed rule
    /// refused to fall back on the name, every rule the user upgraded would
    /// stop working on Windows and for un-bundled binaries.
    #[test]
    fn a_bundle_keyed_rule_still_fires_where_no_bundle_id_is_reported() {
        let rules = [keyed("slack", "com.tinyspeck.slackmacgap")];
        assert!(matching(&rules, Some("Slack"), None).is_some());
    }

    /// Rules written before the field existed have no bundle id and must keep
    /// matching on the name — the whole reason the field is additive.
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
