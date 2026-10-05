//! Proves the Aloud -> Apple Notes copy against the real Notes app: creates a
//! note in a folder called "Aloud test", syncs it again with the id it got
//! (which must update, not add), checks the folder still holds one note with
//! the new text, then deletes the note and the folder. The first run raises
//! Notes' one-time Automation prompt.
//!
//! ```sh
//! cargo run -p aloud-native --example notes_sync
//! ```
//!
//! Notes' `delete` moves things to Recently Deleted, which Notes empties itself
//! after 30 days; this never touches that folder.

use aloud_native::{run_jxa, sync_note, ScriptError};
use serde_json::{json, Value};

const FOLDER: &str = "Aloud test";

/// (note count, bodies) of the test folder, or None when there is no such folder.
fn peek() -> Result<Option<(usize, String)>, ScriptError> {
    let out = run_jxa(
        "Notes",
        r#"const f = Notes_folder(ARG.folder);
        if (!f) return "null";
        return JSON.stringify({ n: f.notes.length, bodies: f.notes().map((n) => n.plaintext()).join(" | ") });
        function Notes_folder(name) {
            const hits = Application("Notes").defaultAccount().folders.whose({ name })();
            return hits.length ? hits[0] : null;
        }"#,
        &json!({ "folder": FOLDER }),
    )?;
    let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    if v.is_null() {
        return Ok(None);
    }
    let count = v["n"].as_u64().unwrap_or(0) as usize;
    Ok(Some((
        count,
        v["bodies"].as_str().unwrap_or("").to_string(),
    )))
}

fn cleanup() -> Result<(), ScriptError> {
    run_jxa(
        "Notes",
        r#"const Notes = Application("Notes");
        for (const f of Notes.defaultAccount().folders.whose({ name: ARG.folder })()) {
            for (const n of f.notes()) Notes.delete(n);
            Notes.delete(f);
        }"#,
        &json!({ "folder": FOLDER }),
    )
    .map(drop)
}

fn run() -> Result<(), String> {
    let e = |e: ScriptError| e.to_string();
    if peek().map_err(e)?.is_some() {
        return Err(format!(
            "a folder named \"{FOLDER}\" already exists; not touching it"
        ));
    }

    let id1 = sync_note(FOLDER, "# Sync test\nfirst version\n- one", None).map_err(e)?;
    println!("created   id={id1}");
    println!("folder    {:?}", peek().map_err(e)?);

    let id2 = sync_note(
        FOLDER,
        "# Sync test\nsecond version\n- one\n- two",
        Some(&id1),
    )
    .map_err(e)?;
    println!("updated   id={id2}");
    let (count, bodies) = peek().map_err(e)?.ok_or("folder vanished")?;
    println!("folder    {count} note(s): {bodies}");

    if id1 != id2 {
        return Err("update changed the note id".into());
    }
    if count != 1 {
        return Err(format!("expected one note in the folder, found {count}"));
    }
    if !bodies.contains("second version") || bodies.contains("first version") {
        return Err("the note was not updated in place".into());
    }
    println!("OK: create then update kept one note, with the new text");
    Ok(())
}

fn main() {
    let result = run();
    match cleanup() {
        Ok(()) => println!("cleanup   deleted the \"{FOLDER}\" folder and its note"),
        Err(err) => println!("cleanup   FAILED: {err}"),
    }
    if let Err(err) = result {
        println!("FAILED: {err}");
        std::process::exit(1);
    }
}
