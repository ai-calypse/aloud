//! A small PNG thumbnail for a file, as a `data:` URI — so the notch grid can
//! draw search results without a second round-trip per image.
//!
//! `sips` for images: it is the same tool `aloud_selection` already uses for app
//! icons, reliable from a background process, and handles jpg/png/heic/tiff. We
//! reach for it rather than `qlmanage` because `qlmanage` silently produces
//! nothing when the Quick Look daemon is not cooperating (verified here from a
//! non-GUI context) — a thumbnail path that works only sometimes is worse than
//! one that only does images.
//!
//! ponytail: images only. Documents fall back to a name tile in the grid. A
//! `qlmanage` (or QuickLookThumbnailing framework) path for PDFs/docs is the
//! upgrade once images prove out — but only if it can be made reliable.

#[cfg(target_os = "macos")]
use std::path::Path;

/// A ~192px PNG preview of `path` as a base64 `data:` URI, or None when `path`
/// is not an image `sips` can read. Off macOS, always None.
pub fn data_uri(path: &str) -> Option<String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
    #[cfg(target_os = "macos")]
    {
        render(Path::new(path))
    }
}

#[cfg(target_os = "macos")]
fn render(path: &Path) -> Option<String> {
    use base64::Engine;
    use std::io::Read;

    if !path.exists() {
        return None;
    }
    // sips writes to a file, so render into a temp path unique to this call so
    // parallel renders never collide on the name.
    let out = std::env::temp_dir().join(format!(
        "aloud-thumb-{}.png",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("x")
            .replace(['/', ' ', '.'], "_")
    ));
    // -Z keeps aspect ratio, longest side to 192. A non-image makes sips fail,
    // which is the signal to fall back to a name tile — not an error.
    let ok = std::process::Command::new("sips")
        .args(["-s", "format", "png", "-Z", "192"])
        .arg(path)
        .arg("--out")
        .arg(&out)
        .output()
        .ok()
        .is_some_and(|o| o.status.success());
    if !ok {
        let _ = std::fs::remove_file(&out);
        return None;
    }

    let mut bytes = Vec::new();
    let read = std::fs::File::open(&out)
        .ok()
        .and_then(|mut f| f.read_to_end(&mut bytes).ok());
    let _ = std::fs::remove_file(&out);
    read?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:image/png;base64,{b64}"))
}
