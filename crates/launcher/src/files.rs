use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// A file the panel can open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct LaunchFile {
    pub path: String,
    pub name: String,
    /// Its folder, `~`-shortened, for the row's subtitle.
    pub folder: String,
}

/// mdfind may return thousands for a short name; the notch shows a handful,
/// and the stat per candidate is the cost.
const CANDIDATES: usize = 80;

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("no home folder")
}

/// Whether a hit is worth a row: a real file in the user's own space. System
/// and app-internal paths (`~/Library`, anything inside a bundle or a hidden
/// folder) match almost every short name and are never what was meant.
fn is_user_file(path: &Path, home: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(home) else {
        return false;
    };
    rel.components().all(|c| {
        let part = c.as_os_str().to_string_lossy();
        !part.starts_with('.')
            && part != "Library"
            && !part.ends_with(".app")
            && !part.ends_with(".photoslibrary")
    })
}

fn describe(path: &Path, home: &Path) -> Option<(SystemTime, LaunchFile)> {
    let meta = std::fs::metadata(path)
        .ok()
        .filter(std::fs::Metadata::is_file)?;
    let folder = path.parent().map(|p| match p.strip_prefix(home) {
        Ok(rel) if rel.as_os_str().is_empty() => "~".to_string(),
        Ok(rel) => format!("~/{}", rel.display()),
        Err(_) => p.display().to_string(),
    });
    Some((
        meta.modified().ok()?,
        LaunchFile {
            path: path.to_string_lossy().into_owned(),
            name: path.file_name()?.to_string_lossy().into_owned(),
            folder: folder.unwrap_or_default(),
        },
    ))
}

fn newest(hits: &str, limit: usize) -> Result<Vec<LaunchFile>> {
    let home = home()?;
    let mut found: Vec<_> = hits
        .lines()
        .map(Path::new)
        .filter(|p| is_user_file(p, &home))
        .take(CANDIDATES)
        .filter_map(|p| describe(p, &home))
        .collect();
    found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    Ok(found.into_iter().take(limit).map(|(_, f)| f).collect())
}

/// Files in the home folder whose name contains `query`, newest first. The
/// query is `mdfind`'s `-name` argument, not part of a predicate, so quotes
/// and operators in it are just characters.
pub fn search_files(query: &str, limit: usize) -> Result<Vec<LaunchFile>> {
    let query = query.trim();
    // mdfind reads a leading "-" as an option and has no `--`; a file name
    // starting with one is rare enough to give up on rather than misrun.
    if query.is_empty() || query.starts_with('-') {
        return Ok(Vec::new());
    }
    let out = Command::new("mdfind")
        .arg("-onlyin")
        .arg(home()?)
        .arg("-name")
        .arg(query)
        .output()
        .context("running mdfind")?;
    newest(&String::from_utf8_lossy(&out.stdout), limit)
}

/// Files opened in the last two weeks, newest first — Spotlight's own
/// last-used date, so it covers whatever app opened them.
pub fn recent_files(limit: usize) -> Result<Vec<LaunchFile>> {
    let out = Command::new("mdfind")
        .arg("-onlyin")
        .arg(home()?)
        .arg("kMDItemLastUsedDate >= $time.today(-14)")
        .output()
        .context("running mdfind")?;
    newest(&String::from_utf8_lossy(&out.stdout), limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_users_own_files_are_worth_a_row() {
        let home = Path::new("/Users/me");
        assert!(is_user_file(Path::new("/Users/me/Documents/plan.md"), home));
        assert!(!is_user_file(Path::new("/Users/me/Library/Caches/x"), home));
        assert!(!is_user_file(Path::new("/Users/me/.config/x"), home));
        assert!(!is_user_file(
            Path::new("/Users/me/Apps/Foo.app/Contents/Info.plist"),
            home
        ));
        assert!(
            !is_user_file(Path::new("/System/Library/x"), home),
            "outside home is never offered"
        );
    }
}
