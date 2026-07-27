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
    // Query is OPTIONAL. "Show me the files on my desktop" is a location listing
    // with no keyword — forcing a query made the model invent one ("any"), which
    // then matched files literally containing that word. None here means "list,
    // don't filter by keyword".
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("any");
    let location = args.get("location").and_then(Value::as_str);

    // A request with neither a keyword, a specific kind, nor a location is not
    // actionable — "find my files" everywhere would return the whole disk. Ask
    // the model to narrow rather than dumping the home folder.
    let has_kind = matches!(kind, "image" | "document" | "screenshot");
    if query.is_none() && location.is_none() && !has_kind {
        return Err(ScriptError::Failed(
            "give a search term, a kind (image/document/screenshot), or a \
             location (desktop/downloads/…) — 'find everything' is too broad"
                .into(),
        ));
    }

    let predicate = build_predicate(query, kind);
    let mut cmd = Command::new("mdfind");
    // Scope: a named location ("desktop", "downloads") narrows to that folder —
    // "on my desktop" must actually mean the Desktop, not everywhere. Otherwise
    // the whole home dir, so /System and app bundles never drown out real files.
    // ponytail: home only for the default; an all-volumes toggle is the upgrade.
    if let Some(scope) = scope_dir(location) {
        cmd.arg("-onlyin").arg(scope);
    } else if let Some(home) = std::env::var_os("HOME") {
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

/// Resolves a named location to an absolute path under `$HOME`, or None for an
/// unknown/absent location (caller then defaults to the whole home dir).
fn scope_dir(location: Option<&str>) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    let sub = match location?.trim().to_ascii_lowercase().as_str() {
        "desktop" => "Desktop",
        "downloads" => "Downloads",
        "documents" => "Documents",
        "pictures" | "photos" => "Pictures",
        "movies" | "videos" => "Movies",
        "music" => "Music",
        _ => return None,
    };
    Some(std::path::Path::new(&home).join(sub))
}

/// A path inside a directory that is machine noise, not the user's content.
/// Matched as `/segment/` so it never trips on a file merely named "build".
fn is_noise(path: &str) -> bool {
    // Substring match on the boundaried segment. Cheap and good enough — a real
    // photo is not inside any of these. `Library/` is the big one: editor
    // backups (Cursor/VSCode History), caches, and app support account for most
    // of the junk a content search dragged in ("9f6U.css" was a Cursor backup).
    const NOISE: &[&str] = &[
        "/Library/",
        "/target/",
        "/node_modules/",
        "/.git/",
        "/.build/",
        "/DerivedData/",
        "/.cargo/",
        "/.rustup/",
        "/.cache/",
        "/.Trash/",
        "/dist/",
        "/build/",
    ];
    NOISE.iter().any(|n| path.contains(n))
}

/// Builds the `mdfind` predicate from an optional `query` and a `kind`.
///
/// Two orthogonal parts, ANDed: a **kind clause** (what type of file) and an
/// optional **keyword clause** (matching the query). With no query it is the
/// kind clause alone — "list the images / screenshots / files here", which is
/// what "show me files on my desktop" means.
///
/// The keyword's match surfaces depend on kind: `kMDItemTextContent` (words
/// *inside* a file) is only OCR-meaningful for **images**. For a `.css`/`.log`,
/// content-match means "this source file mentions the word", never what a person
/// wants — so non-image kinds match filename only.
fn build_predicate(query: Option<&str>, kind: &str) -> String {
    let kind_clause = match kind {
        "image" => "kMDItemContentTypeTree == 'public.image'".to_string(),
        "screenshot" => "kMDItemIsScreenCapture == 1".to_string(),
        "document" => "(kMDItemContentTypeTree == 'public.content' \
                        || kMDItemContentTypeTree == 'com.adobe.pdf')"
            .to_string(),
        // "any": the kinds a person looks for — excludes source/binary files so
        // a common word does not drag in tens of thousands of them.
        _ => "(kMDItemContentTypeTree == 'public.image' \
               || kMDItemContentTypeTree == 'public.audiovisual-content' \
               || kMDItemContentTypeTree == 'public.content' \
               || kMDItemContentTypeTree == 'com.adobe.pdf')"
            .to_string(),
    };

    // No keyword → the kind clause alone lists everything of that kind (in the
    // scoped location). This is the "files on my desktop" case.
    let Some(query) = query else {
        return kind_clause;
    };

    let q = escape_mdfind(query);
    let name = format!("kMDItemDisplayName == '*{q}*'cd");
    let keyword = if kind == "image" {
        // Images also match OCR'd text and scene labels — the whole reason this
        // beats a filename search.
        format!(
            "({name} \
              || kMDItemTextContent == '{q}'cd \
              || kMDItemPhotosSceneClassificationLabels == '{q}'cd \
              || kMDItemPhotosSceneClassificationSynonyms == '{q}'cd)"
        )
    } else {
        // Everything else: filename only.
        name
    };

    format!("({kind_clause} && {keyword})")
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
    fn image_query_matches_name_ocr_and_scene() {
        let p = build_predicate(Some("buddha"), "image");
        assert!(p.contains("kMDItemDisplayName == '*buddha*'cd"));
        // The OCR surface — the whole reason this beats a filename search.
        assert!(p.contains("kMDItemTextContent == 'buddha'cd"));
        assert!(p.contains("SceneClassificationLabels"));
        assert!(p.contains("public.image"));
    }

    #[test]
    fn no_query_lists_the_kind_without_a_keyword() {
        // "files on my desktop" — a listing, not a search. No keyword clause, so
        // nothing is matched against a query; the kind clause stands alone.
        let p = build_predicate(None, "any");
        assert!(
            !p.contains("kMDItemDisplayName"),
            "no keyword expected: {p}"
        );
        assert!(!p.contains("kMDItemTextContent"));
        assert!(p.contains("public.content"));
        // Screenshots with no query = just the capture flag.
        assert_eq!(
            build_predicate(None, "screenshot"),
            "kMDItemIsScreenCapture == 1"
        );
    }

    #[test]
    fn any_kind_matches_filename_only_not_file_content() {
        // The "search screenshot → 40 .css files" bug: a non-image kind must NOT
        // match kMDItemTextContent, or every source file mentioning the word
        // surfaces. Filename only.
        let p = build_predicate(Some("report"), "any");
        assert!(p.contains("kMDItemDisplayName == '*report*'cd"));
        assert!(
            !p.contains("kMDItemTextContent"),
            "any-kind must not content-match: {p}"
        );
        assert!(p.contains("com.adobe.pdf"));
    }

    #[test]
    fn screenshot_kind_uses_the_capture_flag() {
        // "find my screenshots" is a kind (kMDItemIsScreenCapture), never a
        // search for the word "screenshot" inside files.
        let p = build_predicate(Some("error"), "screenshot");
        assert!(p.contains("kMDItemIsScreenCapture == 1"));
        // A query filters by filename, not by content — a screenshot is not
        // OCR-matched here (that path is image-kind).
        assert!(p.contains("kMDItemDisplayName == '*error*'cd"));
        assert!(!p.contains("kMDItemTextContent"), "screenshots: {p}");
    }

    #[test]
    fn document_kind_does_not_content_match() {
        let p = build_predicate(Some("invoice"), "document");
        assert!(p.contains("kMDItemDisplayName == '*invoice*'cd"));
        assert!(!p.contains("kMDItemTextContent"), "docs filename-only: {p}");
    }

    #[test]
    fn named_locations_resolve_under_home() {
        // Guarded on HOME being set (it is, in any real run and in CI).
        if std::env::var_os("HOME").is_some() {
            assert!(scope_dir(Some("desktop")).unwrap().ends_with("Desktop"));
            assert!(scope_dir(Some("Downloads")).unwrap().ends_with("Downloads"));
        }
        // Unknown / absent → None, so the caller falls back to whole-home.
        assert!(scope_dir(Some("nonsense")).is_none());
        assert!(scope_dir(None).is_none());
    }

    #[test]
    fn library_backups_are_noise() {
        // The exact junk the user saw: a Cursor editor backup in Library.
        assert!(is_noise(
            "/Users/x/Library/Application Support/Cursor/User/History/9f6U.css"
        ));
        // But the Photos library (a bundle, not /Library/) is real content.
        assert!(!is_noise(
            "/Users/x/Pictures/Photos Library.photoslibrary/originals/a.heic"
        ));
    }

    #[test]
    fn query_with_apostrophe_is_escaped() {
        // "st patrick's" must not truncate the predicate at the apostrophe.
        let p = build_predicate(Some("patrick's"), "any");
        assert!(p.contains(r"patrick\'s"), "apostrophe must be escaped: {p}");
    }

    #[test]
    fn too_broad_a_request_is_rejected_but_a_location_alone_is_fine() {
        // No keyword, no kind, no location → "find everything", rejected so the
        // model narrows instead of dumping the disk.
        assert!(run(&json!({})).is_err());
        assert!(run(&json!({ "query": "  " })).is_err());
        // But a location alone IS actionable — "files on my desktop" lists it.
        // (Runs a real mdfind; just assert it does not error.)
        assert!(run(&json!({ "location": "desktop" })).is_ok());
        // A kind alone is actionable too — "my screenshots".
        assert!(run(&json!({ "kind": "screenshot" })).is_ok());
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
