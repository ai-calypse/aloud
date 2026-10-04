//! Thumbnails as `data:` URIs: the overlay's CSP loads no `file:` or `asset:`
//! image outside the media folder, and a row's picture is a few KB anyway.
//! `sips` makes them (macOS ships it), cached on disk so a screenshot list
//! costs one `sips` per picture, ever.
//!
//! ponytail: the cache is never pruned. A thumbnail is ~3 KB; a sweep by age
//! is the upgrade if the folder ever matters.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use anyhow::{Context, Result};
use base64::Engine;

use crate::run;

/// A JPEG `px` on its long side, as a `data:` URI.
pub fn data_uri(cache: &Path, src: &Path, px: u32) -> Result<String> {
    let meta =
        std::fs::metadata(src).with_context(|| format!("no such file: {}", src.display()))?;
    // A picture edited in place keeps its path but not its thumbnail.
    let mut h = DefaultHasher::new();
    (src, meta.len(), crate::mtime_secs(src), px).hash(&mut h);
    let out = cache.join(format!("{:016x}.jpg", h.finish()));
    if !out.exists() {
        std::fs::create_dir_all(cache)?;
        // Beside and renamed, so a killed `sips` never leaves a half image
        // that the next call would serve as the thumbnail.
        let tmp = out.with_extension("part");
        let made = run(
            "sips",
            &[
                "-Z".as_ref(),
                px.to_string().as_ref(),
                "-s".as_ref(),
                "format".as_ref(),
                "jpeg".as_ref(),
                src.as_os_str(),
                "--out".as_ref(),
                tmp.as_os_str(),
            ],
        );
        if let Err(e) = made {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.context("could not make a thumbnail"));
        }
        std::fs::rename(&tmp, &out)?;
    }
    let bytes = std::fs::read(&out)?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Is `path` inside one of `roots`? Both sides are resolved first, so `..`
/// and a symlink out of a root do not slip past a prefix check. The webview
/// names the paths it wants thumbnails for; it gets pictures from the
/// folders the Shelf shows and nowhere else.
pub fn within(path: &Path, roots: &[std::path::PathBuf]) -> bool {
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    roots
        .iter()
        .filter_map(|r| r.canonicalize().ok())
        .any(|r| path.starts_with(r))
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[test]
    fn dotdot_and_symlinks_cannot_leave_a_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("shots");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.png"), "x").unwrap();
        std::fs::write(dir.path().join("secret.png"), "x").unwrap();
        std::os::unix::fs::symlink(dir.path().join("secret.png"), root.join("link.png")).unwrap();
        let roots = [root.clone()];

        assert!(within(&root.join("a.png"), &roots));
        assert!(!within(&root.join("../secret.png"), &roots));
        assert!(!within(&root.join("link.png"), &roots));
        assert!(!within(&root.join("missing.png"), &roots));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn a_picture_gets_a_jpeg_data_uri_and_a_second_call_reuses_it() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("dot.png");
        std::fs::write(&png, include_bytes!("../tests/dot.png")).unwrap();
        let cache = dir.path().join("thumbs");
        let a = data_uri(&cache, &png, 64).unwrap();
        assert!(a.starts_with("data:image/jpeg;base64,"));
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);
        assert_eq!(data_uri(&cache, &png, 64).unwrap(), a);
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);
    }

    #[test]
    fn a_non_picture_is_an_error_not_an_empty_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("a.txt");
        std::fs::write(&txt, "hi").unwrap();
        assert!(data_uri(&dir.path().join("t"), &txt, 64).is_err());
        assert!(!dir
            .path()
            .join("t")
            .read_dir()
            .unwrap()
            .any(|e| { e.unwrap().path().extension().is_some_and(|x| x == "jpg") }));
    }
}
