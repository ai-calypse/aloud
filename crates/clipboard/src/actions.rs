//! Doing something with an entry: copy it back, or run one of its smart
//! actions. Everything lands on the clipboard through `aloud_paste`, which
//! marks it as Aloud's own write so the history does not record its own output.

use anyhow::{bail, Context, Result};
use std::process::Command;

use crate::colors;
use crate::entry::reformat_json;
use crate::store::{Content, Store};

/// Puts an entry back on the clipboard.
pub fn copy(store: &Store, id: u32) -> Result<()> {
    match store.content(id)?.context("that entry is gone")? {
        Content::Text(text) => aloud_paste::set_clipboard(&text),
        Content::Files(files) => aloud_paste::set_clipboard(&files.join("\n")),
        Content::Image(path) => {
            let img = image::open(&path)
                .context("reading the stored image")?
                .to_rgba8();
            let (w, h) = (img.width() as usize, img.height() as usize);
            aloud_paste::set_clipboard_image(w, h, img.into_raw())
        }
    }
}

fn open_target(target: &str) -> Result<()> {
    // `open` takes the target as one argument, never through a shell.
    let status = Command::new("open")
        .arg(target)
        .status()
        .context("running open")?;
    anyhow::ensure!(status.success(), "open could not open that");
    Ok(())
}

/// OCR on a file path, for the live example: it has no stored entry to name.
#[doc(hidden)]
pub fn run_ocr_for_example(path: &std::path::Path) -> Result<String> {
    crate::ocr::read(path)
}

/// Runs a smart action by id and says what it did, in a few words.
pub fn run(store: &Store, id: u32, action: &str) -> Result<String> {
    let content = store.content(id)?.context("that entry is gone")?;
    match (&content, action) {
        (Content::Text(text), "open-url") => {
            let url = text.trim();
            // Only web links: `open` would also launch an app or a file given
            // a `file:` or custom-scheme "link" copied from anywhere.
            anyhow::ensure!(
                url.starts_with("http://") || url.starts_with("https://"),
                "not a web link"
            );
            open_target(url)?;
            Ok("Opened".into())
        }
        (Content::Text(text), "compose-email") => {
            open_target(&format!("mailto:{}", text.trim()))?;
            Ok("Opened".into())
        }
        (Content::Text(text), "format-json" | "minify-json") => {
            let out = reformat_json(text, action == "format-json").context("not valid JSON")?;
            aloud_paste::set_clipboard(&out)?;
            Ok("Copied".into())
        }
        (Content::Text(text), "trim") => {
            aloud_paste::set_clipboard(text.trim())?;
            Ok("Copied".into())
        }
        (Content::Text(text), "color-hex" | "color-rgb") => {
            let rgb = colors::parse(text).context("not a colour")?;
            aloud_paste::set_clipboard(&if action == "color-hex" {
                rgb.hex()
            } else {
                rgb.css()
            })?;
            Ok("Copied".into())
        }
        (Content::Files(files), "reveal") => {
            let first = files.first().context("no files")?;
            let status = Command::new("open")
                .arg("-R")
                .arg(first)
                .status()
                .context("running open")?;
            anyhow::ensure!(status.success(), "could not show that in Finder");
            Ok("Shown".into())
        }
        (Content::Files(files), "copy-paths") => {
            aloud_paste::set_clipboard(&files.join("\n"))?;
            Ok("Copied".into())
        }
        (Content::Image(path), "ocr") => {
            let text = crate::ocr::read(path)?;
            anyhow::ensure!(!text.trim().is_empty(), "no text found in the image");
            aloud_paste::set_clipboard(&text)?;
            Ok("Copied text".into())
        }
        _ => bail!("that action does not apply here"),
    }
}
