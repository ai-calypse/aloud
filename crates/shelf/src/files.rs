//! The file shelf: things parked on the notch to be dragged out later.
//!
//! Each item is a *copy* under the shelf's own folder, not a reference. A
//! reference to a file that is later moved, renamed or on an unplugged drive
//! drags out as nothing, and the point of a shelf is that it still has the
//! thing when you get to the other app. `ditto` does the copy because it
//! takes folders too and keeps metadata.
//!
//! ponytail: a full copy per item, so a 20 GB movie costs 20 GB. APFS clones
//! are free when the shelf is on the same volume (`cp -c`); switch to that
//! when someone parks something big.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::run;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    File,
    Folder,
}

/// One thing on the shelf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[serde(rename = "ShelfItem")]
pub struct Item {
    pub id: String,
    /// Shown, and what a drag out is called.
    pub name: String,
    /// The shelf's copy — what is dragged out and shared.
    pub path: String,
    /// Where it came from. Dropping the same original again replaces the copy
    /// rather than shelving a stale twin.
    pub original: String,
    pub kind: Kind,
    /// Bytes; a folder's is its own entry size, not its contents'.
    #[specta(type = specta_typescript::Number)]
    pub size: u64,
    /// Unix seconds when it was shelved.
    #[specta(type = specta_typescript::Number)]
    pub added: i64,
    /// A picture `sips` can convert and the notch can preview.
    pub image: bool,
}

/// What `sips` can write. Declared with its extension, so a name and a format
/// never disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ShelfImageFormat {
    Png,
    Jpeg,
    Heic,
    Tiff,
}

impl ShelfImageFormat {
    fn sips(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Heic => "heic",
            Self::Tiff => "tiff",
        }
    }

    fn ext(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            other => other.sips(),
        }
    }
}

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "heic", "heif", "tif", "tiff", "gif", "bmp", "webp",
];

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// The shelf on disk: `dir/items.json` plus `dir/files/<id>/<name>`.
pub struct Shelf {
    dir: PathBuf,
    items: Mutex<Vec<Item>>,
    counter: AtomicU64,
}

impl Shelf {
    /// Opens the shelf, forgetting items whose files were deleted behind its
    /// back (a cleaner app, a manual `rm`): a row that drags out nothing is
    /// worse than no row.
    pub fn open(dir: PathBuf) -> Self {
        let mut items: Vec<Item> = std::fs::read(dir.join("items.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        items.retain(|i| Path::new(&i.path).exists());
        Self {
            dir,
            items: Mutex::new(items),
            counter: AtomicU64::new(0),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Item>> {
        // A panic while holding it must not take the shelf with it.
        self.items
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn save(&self, items: &[Item]) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        // Written beside and renamed over, so a crash mid-write leaves the
        // old list rather than half of a new one.
        let tmp = self.dir.join("items.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(items)?)?;
        std::fs::rename(tmp, self.dir.join("items.json"))?;
        Ok(())
    }

    /// Newest first.
    pub fn items(&self) -> Vec<Item> {
        self.lock().clone()
    }

    pub fn item(&self, id: &str) -> Option<Item> {
        self.lock().iter().find(|i| i.id == id).cloned()
    }

    /// Paths of the shelf's copies, for sharing: errors on an unknown id
    /// rather than quietly sharing fewer files than were asked for.
    pub fn paths(&self, ids: &[String]) -> Result<Vec<PathBuf>> {
        ids.iter()
            .map(|id| {
                self.item(id)
                    .map(|i| PathBuf::from(i.path))
                    .ok_or_else(|| anyhow!("that file is no longer on the shelf"))
            })
            .collect()
    }

    fn new_id(&self) -> String {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        format!("{ms}-{}", self.counter.fetch_add(1, Ordering::Relaxed))
    }

    /// A fresh `files/<id>/` to build one item in.
    fn slot(&self) -> Result<(String, PathBuf)> {
        let id = self.new_id();
        let dir = self.dir.join("files").join(&id);
        std::fs::create_dir_all(&dir)?;
        Ok((id, dir))
    }

    fn record(&self, id: String, path: PathBuf, original: &str) -> Result<Item> {
        let meta = std::fs::metadata(&path)?;
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(|| anyhow!("that file has no name"))?
            .to_string();
        let item = Item {
            id,
            image: !meta.is_dir() && is_image(&path),
            kind: if meta.is_dir() {
                Kind::Folder
            } else {
                Kind::File
            },
            size: meta.len(),
            added: crate::mtime_secs_now(),
            path: path.to_string_lossy().into_owned(),
            original: original.to_string(),
            name,
        };
        let mut items = self.lock();
        // Dropping a file again means "this version", so it replaces the old.
        let stale: Vec<Item> = items
            .iter()
            .filter(|i| i.original == item.original)
            .cloned()
            .collect();
        items.retain(|i| i.original != item.original);
        items.insert(0, item.clone());
        self.save(&items)?;
        drop(items);
        for old in stale {
            self.discard(&old);
        }
        Ok(item)
    }

    /// Deletes an item's folder. Only ever the shelf's own `files/<id>`:
    /// `items.json` is data the shelf wrote, but a hand-edited path must not
    /// make "remove" delete something outside it.
    fn discard(&self, item: &Item) {
        let root = self.dir.join("files");
        let slot = root.join(&item.id);
        if slot.starts_with(&root) && item.id.chars().all(|c| c.is_ascii_digit() || c == '-') {
            let _ = std::fs::remove_dir_all(slot);
        }
    }

    /// Parks copies of `paths`. A path that cannot be copied is skipped and
    /// reported, the rest still land: a drop of ten files should not be lost
    /// to one unreadable one.
    pub fn add(&self, paths: &[PathBuf]) -> (Vec<Item>, Vec<String>) {
        let mut added = Vec::new();
        let mut failed = Vec::new();
        for path in paths {
            // Dropping a row back onto the shelf it was dragged from: nothing
            // to do, and not worth an error.
            if path.starts_with(self.dir.join("files")) {
                continue;
            }
            match self.add_one(path) {
                Ok(item) => added.push(item),
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "could not shelve");
                    failed.push(format!("{}: {e}", path.display()));
                }
            }
        }
        (added, failed)
    }

    fn add_one(&self, src: &Path) -> Result<Item> {
        let name = src
            .file_name()
            .ok_or_else(|| anyhow!("that path has no name"))?;
        let (id, slot) = self.slot()?;
        let dst = slot.join(name);
        if let Err(e) = copy(src, &dst) {
            let _ = std::fs::remove_dir_all(&slot);
            return Err(e);
        }
        self.record(id, dst, &src.to_string_lossy())
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut items = self.lock();
        let Some(at) = items.iter().position(|i| i.id == id) else {
            return Ok(());
        };
        let gone = items.remove(at);
        self.save(&items)?;
        drop(items);
        self.discard(&gone);
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        let mut items = self.lock();
        let all = std::mem::take(&mut *items);
        self.save(&items)?;
        drop(items);
        for item in all {
            self.discard(&item);
        }
        Ok(())
    }

    /// Converts a shelved picture and shelves the result beside it.
    pub fn convert(&self, id: &str, to: ShelfImageFormat) -> Result<Item> {
        let src = self
            .item(id)
            .ok_or_else(|| anyhow!("that file is no longer on the shelf"))?;
        if !src.image {
            bail!("{} is not a picture", src.name);
        }
        let stem = Path::new(&src.name)
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("image");
        let (new_id, slot) = self.slot()?;
        let dst = slot.join(format!("{stem}.{}", to.ext()));
        let result = run(
            "sips",
            &[
                "-s".as_ref(),
                "format".as_ref(),
                to.sips().as_ref(),
                src.path.as_ref(),
                "--out".as_ref(),
                dst.as_os_str(),
            ],
        );
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&slot);
            return Err(e.context("could not convert the picture"));
        }
        // Its own original, so shelving it never replaces the source.
        self.record(new_id, dst.clone(), &dst.to_string_lossy())
    }

    /// Zips shelved items into one archive and shelves it. One item is zipped
    /// under its own name; several go under a folder called `Archive`, so
    /// unzipping does not spill loose files into the destination.
    pub fn zip(&self, ids: &[String]) -> Result<Item> {
        let sources: Vec<Item> = ids
            .iter()
            .map(|id| {
                self.item(id)
                    .ok_or_else(|| anyhow!("that file is no longer on the shelf"))
            })
            .collect::<Result<_>>()?;
        let (new_id, slot) = self.slot()?;
        let built = (|| -> Result<PathBuf> {
            let (input, name) = match sources.as_slice() {
                [] => bail!("nothing to zip"),
                [one] => (PathBuf::from(&one.path), one.name.clone()),
                many => {
                    let stage = slot.join("stage").join("Archive");
                    std::fs::create_dir_all(&stage)?;
                    for item in many {
                        copy(Path::new(&item.path), &stage.join(&item.name))?;
                    }
                    (stage, "Archive".to_string())
                }
            };
            let out = slot.join(format!("{name}.zip"));
            run(
                "ditto",
                &[
                    "-c".as_ref(),
                    "-k".as_ref(),
                    "--keepParent".as_ref(),
                    input.as_os_str(),
                    out.as_os_str(),
                ],
            )
            .context("could not zip")?;
            let _ = std::fs::remove_dir_all(slot.join("stage"));
            Ok(out)
        })();
        match built {
            Ok(out) => self.record(new_id, out.clone(), &out.to_string_lossy()),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&slot);
                Err(e)
            }
        }
    }

    /// Unzips a shelved archive into a folder named after it, and shelves
    /// the folder.
    pub fn unzip(&self, id: &str) -> Result<Item> {
        let src = self
            .item(id)
            .ok_or_else(|| anyhow!("that file is no longer on the shelf"))?;
        let path = Path::new(&src.name);
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        {
            bail!("{} is not a zip", src.name);
        }
        let stem = path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("Unzipped");
        let (new_id, slot) = self.slot()?;
        let out = slot.join(stem);
        let result = std::fs::create_dir_all(&out)
            .map_err(anyhow::Error::from)
            .and_then(|()| {
                run(
                    "ditto",
                    &[
                        "-x".as_ref(),
                        "-k".as_ref(),
                        src.path.as_ref(),
                        out.as_os_str(),
                    ],
                )
            });
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&slot);
            return Err(e.context("could not unzip"));
        }
        self.record(new_id, out.clone(), &out.to_string_lossy())
    }
}

/// `ditto` copies a file or a whole folder, keeping its metadata.
fn copy(src: &Path, dst: &Path) -> Result<()> {
    run("ditto", &[src.as_os_str(), dst.as_os_str()]).context("could not copy")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shelf() -> (tempfile::TempDir, Shelf) {
        let dir = tempfile::tempdir().unwrap();
        let shelf = Shelf::open(dir.path().join("shelf"));
        (dir, shelf)
    }

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn a_shelved_file_survives_its_original_being_deleted() {
        let (tmp, shelf) = shelf();
        let src = write(tmp.path(), "notes.txt", "hello");
        let (added, failed) = shelf.add(std::slice::from_ref(&src));
        assert!(failed.is_empty());
        std::fs::remove_file(&src).unwrap();
        let item = &shelf.items()[0];
        assert_eq!(std::fs::read_to_string(&item.path).unwrap(), "hello");
        assert_eq!(added[0].name, "notes.txt");
    }

    #[test]
    fn dropping_a_file_again_replaces_the_stale_copy() {
        let (tmp, shelf) = shelf();
        let src = write(tmp.path(), "a.txt", "v1");
        shelf.add(std::slice::from_ref(&src));
        let first = shelf.items()[0].clone();
        std::fs::write(&src, "v2").unwrap();
        shelf.add(&[src]);
        let items = shelf.items();
        assert_eq!(items.len(), 1, "no stale twin");
        assert_eq!(std::fs::read_to_string(&items[0].path).unwrap(), "v2");
        assert!(!Path::new(&first.path).exists(), "old copy is deleted");
    }

    #[test]
    fn one_unreadable_drop_does_not_lose_the_others() {
        let (tmp, shelf) = shelf();
        let ok = write(tmp.path(), "ok.txt", "x");
        let (added, failed) = shelf.add(&[tmp.path().join("missing.txt"), ok]);
        assert_eq!(added.len(), 1);
        assert_eq!(failed.len(), 1);
        // And the failed attempt left no empty folder behind.
        let slots = std::fs::read_dir(tmp.path().join("shelf/files"))
            .unwrap()
            .count();
        assert_eq!(slots, 1);
    }

    #[test]
    fn the_shelf_comes_back_after_a_relaunch_minus_what_vanished() {
        let (tmp, shelf) = shelf();
        shelf.add(&[
            write(tmp.path(), "keep.txt", "k"),
            write(tmp.path(), "lose.txt", "l"),
        ]);
        let lose = shelf
            .items()
            .into_iter()
            .find(|i| i.name == "lose.txt")
            .unwrap();
        std::fs::remove_file(&lose.path).unwrap();

        let reopened = Shelf::open(tmp.path().join("shelf"));
        let names: Vec<_> = reopened.items().into_iter().map(|i| i.name).collect();
        assert_eq!(names, ["keep.txt"]);
    }

    #[test]
    fn remove_deletes_the_copy_and_only_the_copy() {
        let (tmp, shelf) = shelf();
        let src = write(tmp.path(), "a.txt", "x");
        shelf.add(std::slice::from_ref(&src));
        let item = shelf.items()[0].clone();
        shelf.remove(&item.id).unwrap();
        assert!(shelf.items().is_empty());
        assert!(!Path::new(&item.path).exists());
        assert!(src.exists());
    }

    #[test]
    fn a_hand_edited_id_cannot_delete_outside_the_shelf() {
        let (tmp, shelf) = shelf();
        let victim = tmp.path().join("precious");
        std::fs::create_dir_all(&victim).unwrap();
        let item = Item {
            id: "../../precious".into(),
            name: "x".into(),
            path: String::new(),
            original: String::new(),
            kind: Kind::File,
            size: 0,
            added: 0,
            image: false,
        };
        shelf.discard(&item);
        assert!(victim.exists());
    }

    #[test]
    fn shelving_a_folder_keeps_its_contents() {
        let (tmp, shelf) = shelf();
        let dir = tmp.path().join("proj");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        write(&dir.join("sub"), "f.txt", "deep");
        shelf.add(&[dir]);
        let item = &shelf.items()[0];
        assert_eq!(item.kind, Kind::Folder);
        assert_eq!(
            std::fs::read_to_string(Path::new(&item.path).join("sub/f.txt")).unwrap(),
            "deep"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn zip_then_unzip_round_trips_several_files_under_one_folder() {
        let (tmp, shelf) = shelf();
        shelf.add(&[
            write(tmp.path(), "a.txt", "A"),
            write(tmp.path(), "b.txt", "B"),
        ]);
        let ids: Vec<String> = shelf.items().into_iter().map(|i| i.id).collect();
        let zip = shelf.zip(&ids).unwrap();
        assert_eq!(zip.name, "Archive.zip");
        let out = shelf.unzip(&zip.id).unwrap();
        assert_eq!(out.name, "Archive");
        // keepParent: the folder is inside the unzip folder, so it does not spill.
        assert_eq!(
            std::fs::read_to_string(Path::new(&out.path).join("Archive/a.txt")).unwrap(),
            "A"
        );
        assert_eq!(
            std::fs::read_to_string(Path::new(&out.path).join("Archive/b.txt")).unwrap(),
            "B"
        );
        assert!(!Path::new(&zip.path).with_file_name("stage").exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn convert_makes_a_real_jpeg_and_keeps_the_source() {
        let (tmp, shelf) = shelf();
        // A 1x1 PNG.
        let png = tmp.path().join("dot.png");
        std::fs::write(&png, include_bytes!("../tests/dot.png")).unwrap();
        shelf.add(&[png]);
        let id = shelf.items()[0].id.clone();
        let jpg = shelf.convert(&id, ShelfImageFormat::Jpeg).unwrap();
        assert_eq!(jpg.name, "dot.jpg");
        let head = std::fs::read(&jpg.path).unwrap();
        assert_eq!(&head[..2], [0xFF, 0xD8], "JPEG magic");
        assert_eq!(shelf.items().len(), 2, "source stays");
    }

    #[test]
    fn a_text_file_is_not_convertible() {
        let (tmp, shelf) = shelf();
        shelf.add(&[write(tmp.path(), "a.txt", "x")]);
        let id = shelf.items()[0].id.clone();
        assert!(shelf.convert(&id, ShelfImageFormat::Png).is_err());
    }
}
