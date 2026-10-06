//! Connected services' logos, for the approval card and the toast.
//!
//! The overlay loads only `data:` images (its CSP), so a toolkit's remote logo
//! is fetched once, kept on disk, and handed to the panel as a data URI. The
//! card never waits on a fetch: a logo not on disk yet is a neutral glyph this
//! time and the real one from then on.
//!
//! ponytail: a kept logo is never refreshed — deleting the folder is the
//! refresh, and a rebrand is rare enough for that.

use crate::ComposioClient;
use std::path::{Path, PathBuf};

/// The logo kept for `toolkit` under `dir`, as a data URI. Never fetches.
pub fn cached(dir: &Path, toolkit: &str) -> Option<String> {
    data_uri(&std::fs::read(file(dir, toolkit)?).ok()?)
}

/// Fetches and keeps the logos of `toolkits` not on disk yet. A failure is
/// logged and skipped: a missing logo costs a glyph, never a card.
pub async fn fetch_missing(dir: &Path, composio: &ComposioClient, toolkits: &[String]) {
    for toolkit in toolkits {
        let Some(path) = file(dir, toolkit).filter(|path| !path.exists()) else {
            continue;
        };
        match composio.logo(toolkit).await {
            Ok(bytes) if data_uri(&bytes).is_some() => {
                if let Err(e) =
                    std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&path, bytes))
                {
                    tracing::warn!(%e, toolkit, "could not keep a logo");
                }
            }
            Ok(_) => tracing::debug!(toolkit, "a toolkit's logo is not an image"),
            Err(e) => tracing::debug!(%e, toolkit, "no logo for a toolkit"),
        }
    }
}

/// Where a toolkit's logo is kept. None for anything but a plain slug, which
/// is all a file here is ever named from.
fn file(dir: &Path, toolkit: &str) -> Option<PathBuf> {
    let plain = !toolkit.is_empty()
        && toolkit
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    plain.then(|| dir.join(toolkit.to_ascii_lowercase()))
}

/// An image as a data URI, its type read from its own bytes — what the logo
/// host claimed it was is not trusted, and anything not an image is refused.
fn data_uri(bytes: &[u8]) -> Option<String> {
    use base64::Engine;
    let mime = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(b"\xFF\xD8\xFF") {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).contains("<svg") {
        // Safe as an <img>: an SVG loaded that way runs no script.
        "image/svg+xml"
    } else {
        return None;
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{mime};base64,{encoded}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aloud-icons-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_kept_logo_comes_back_as_a_data_uri_and_a_missing_one_as_nothing() {
        let dir = scratch("kept");
        assert_eq!(cached(&dir, "gmail"), None, "nothing kept, nothing fetched");

        std::fs::create_dir_all(&dir).unwrap();
        let png = b"\x89PNG\r\n\x1a\nrest-of-a-png";
        std::fs::write(dir.join("gmail"), png).unwrap();
        let uri = cached(&dir, "gmail").expect("kept");
        assert!(uri.starts_with("data:image/png;base64,"));
        // Kept by slug, whatever case the toolkit arrives in.
        assert_eq!(cached(&dir, "GMAIL"), Some(uri));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A slug names a file here, so one that is not a plain name names none.
    #[test]
    fn a_slug_that_is_a_path_is_never_a_file() {
        let dir = Path::new("/tmp/icons");
        assert_eq!(
            file(dir, "microsoft_teams"),
            Some(dir.join("microsoft_teams"))
        );
        assert_eq!(file(dir, "../settings.json"), None);
        assert_eq!(file(dir, "a/b"), None);
        assert_eq!(file(dir, ""), None);
    }

    #[test]
    fn only_an_image_becomes_a_data_uri() {
        let svg = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"/>"#;
        assert!(data_uri(svg)
            .unwrap()
            .starts_with("data:image/svg+xml;base64,"));
        assert!(data_uri(b"\xFF\xD8\xFF\xE0jpeg")
            .unwrap()
            .starts_with("data:image/jpeg"));
        assert!(data_uri(b"RIFF\0\0\0\0WEBPVP8 ")
            .unwrap()
            .starts_with("data:image/webp"));
        // An error page served where the logo should be.
        assert_eq!(data_uri(b"<!doctype html><title>404</title>"), None);
        assert_eq!(data_uri(b""), None);
    }
}
