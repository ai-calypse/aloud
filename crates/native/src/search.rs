//! Spotlight search — the "agent for your Mac" tool. Finds files by name, by
//! **text seen inside images** (macOS Live Text OCR, `kMDItemTextContent`), by
//! scene/object labels (`kMDItemPhotosSceneClassificationLabels`, populated for
//! Photos-library images), and by kind.
//!
//! Verified on macOS 26: `mdfind` matching an image's OCR'd text works on any
//! file on disk — "find my screenshots with the word invoice" resolves
//! instantly. Scene labels only exist for images Photos has analysed, so object
//! recognition on loose files degrades to name+OCR; the query still lights up
//! automatically wherever labels are present.
//!
//! ponytail: mdfind only. A vision-model fallback for true object recognition on
//! un-analysed images is the upgrade path once this proves out — it needs a
//! candidate set and token budget, so it is a separate build.

use std::process::Command;

use serde_json::{json, Value};

use crate::script::ScriptError;

/// Cap the result set. A grid shows a handful; an unbounded `mdfind` on a broad
/// term ("photo") can return thousands, none of which the notch can render.
const MAX_RESULTS: usize = 60;

/// Runs a Spotlight search for `query`, optionally restricted to a kind. Returns
/// `{ query, count, results: [{ path, name }] }` — paths only, no thumbnails;
/// the app layer turns paths into thumbnails where it has `sips` and an
/// `AppHandle`. Read-only: it looks, it does not touch anything.
pub fn run(args: &Value) -> Result<Value, ScriptError> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ScriptError::Failed("search needs a query".into()))?;

    // `kind` narrows the content-type tree. Default "any" keeps the "all files"
    // promise; "image" is the highest-signal case and the one the grid renders.
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("any");

    let predicate = build_predicate(query, kind);
    let mut cmd = Command::new("mdfind");
    // Scope to the user's home. Without this, a query that happens to be a
    // dev-project or app name ("aloud") drowns in /System and bundle internals;
    // home is where a person's own files — photos, screenshots, documents —
    // actually live. ponytail: home only; an all-volumes toggle is the upgrade
    // if someone keeps files outside it.
    if let Some(home) = std::env::var_os("HOME") {
        cmd.arg("-onlyin").arg(home);
    }
    let out = cmd
        .arg(&predicate)
        .output()
        .map_err(|e| ScriptError::Failed(format!("running mdfind: {e}")))?;
    if !out.status.success() {
        return Err(ScriptError::Failed(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let paths: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        // Drop machine noise: build output, dependencies, VCS internals, caches,
        // and app support. These match by name constantly (a repo named after
        // the query fills `target/` with thousands of hits) and are never what
        // "find my …" means.
        .filter(|path| !is_noise(path))
        .collect();

    let total = paths.len();
    let results: Vec<Value> = paths
        .iter()
        .take(MAX_RESULTS)
        .map(|path| {
            let name = path.rsplit('/').next().unwrap_or(path);
            json!({ "path": path, "name": name })
        })
        .collect();

    Ok(json!({
        "query": query,
        "count": total,
        "truncated": total > MAX_RESULTS,
        "results": results,
    }))
}

/// A path inside a directory that is machine noise, not the user's content.
/// Matched as `/segment/` so it never trips on a file merely named "build".
fn is_noise(path: &str) -> bool {
    // Substring match on the boundaried segment. Cheap and good enough — a real
    // photo is not inside any of these.
    const NOISE: &[&str] = &[
        "/target/",
        "/node_modules/",
        "/.git/",
        "/.build/",
        "/DerivedData/",
        "/Library/Caches/",
        "/Library/Developer/",
        "/.cargo/",
        "/.rustup/",
        "/.cache/",
        "/dist/",
        "/build/",
    ];
    NOISE.iter().any(|n| path.contains(n))
}

/// Builds the `mdfind` predicate. Escapes the query for the quoted literal,
/// then ORs the match surfaces: display name (substring), OCR'd text content
/// (word), and scene labels (word). `cd` = case- and diacritic-insensitive.
fn build_predicate(query: &str, kind: &str) -> String {
    let q = escape_mdfind(query);
    let matches = format!(
        "(kMDItemDisplayName == '*{q}*'cd \
         || kMDItemTextContent == '{q}'cd \
         || kMDItemPhotosSceneClassificationLabels == '{q}'cd \
         || kMDItemPhotosSceneClassificationSynonyms == '{q}'cd)"
    );
    match kind {
        "image" => format!("(kMDItemContentTypeTree == 'public.image' && {matches})"),
        "document" => format!(
            "((kMDItemContentTypeTree == 'public.content' \
              || kMDItemContentTypeTree == 'com.adobe.pdf') && {matches})"
        ),
        // "any" is NOT truly unrestricted: an unrestricted name match on a
        // common word drags in tens of thousands of source and binary files
        // (a repo named after the query alone fills `target/` with them), which
        // is both slow and never what "find my …" means. Restrict to the kinds a
        // person actually looks for — images, media, PDFs, and real documents.
        _ => format!(
            "((kMDItemContentTypeTree == 'public.image' \
              || kMDItemContentTypeTree == 'public.audiovisual-content' \
              || kMDItemContentTypeTree == 'public.content' \
              || kMDItemContentTypeTree == 'com.adobe.pdf') && {matches})"
        ),
    }
}

/// Escapes a query for an `mdfind` single-quoted literal. Only `'` and `\` are
/// special inside one; without this a query with an apostrophe truncates the
/// predicate.
fn escape_mdfind(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicate_matches_name_ocr_and_scene() {
        let p = build_predicate("buddha", "image");
        assert!(p.contains("kMDItemDisplayName == '*buddha*'cd"));
        // The OCR surface — the whole reason this beats a filename search.
        assert!(p.contains("kMDItemTextContent == 'buddha'cd"));
        assert!(p.contains("SceneClassificationLabels"));
        assert!(p.contains("public.image"));
    }

    #[test]
    fn any_kind_restricts_to_user_content_not_everything() {
        // "any" must exclude source/binary files (the 145k-result, 48s
        // pathological case) by restricting to content kinds — while still
        // spanning more than images.
        let p = build_predicate("report", "any");
        assert!(p.contains("kMDItemTextContent == 'report'cd"));
        assert!(p.contains("com.adobe.pdf"));
        assert!(p.contains("public.content"));
    }

    #[test]
    fn query_with_apostrophe_is_escaped() {
        // "st patrick's" must not truncate the predicate at the apostrophe.
        let p = build_predicate("patrick's", "any");
        assert!(p.contains(r"patrick\'s"), "apostrophe must be escaped: {p}");
    }

    #[test]
    fn empty_query_is_rejected() {
        assert!(run(&json!({ "query": "  " })).is_err());
        assert!(run(&json!({})).is_err());
    }

    #[test]
    fn noise_dirs_are_dropped_but_real_files_kept() {
        // The bug the user hit: a repo named after the query fills target/ with
        // thousands of matches that bury real content.
        assert!(is_noise(
            "/Users/x/Development/aloud-tauri/target/debug/aloud.d"
        ));
        assert!(is_noise("/Users/x/proj/node_modules/aloud/index.js"));
        assert!(is_noise("/Users/x/Library/Caches/aloud/thumb.png"));
        // A real screenshot survives.
        assert!(!is_noise("/Users/x/Desktop/Screenshot aloud.png"));
        assert!(!is_noise("/Users/x/Pictures/buddha-temple.jpg"));
        // A file merely named "build" is not in a build dir.
        assert!(!is_noise("/Users/x/Documents/build-plan.pdf"));
    }
}
