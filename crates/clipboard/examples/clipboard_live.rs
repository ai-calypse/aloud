// Exercises the real pasteboard: what the watcher records, what it must not
// (Aloud's own writes, a concealed copy), and the copy-back. The user's
// clipboard is saved first and restored at the end.
//
//   cargo run -p aloud-clipboard --example clipboard_live [image-with-text.png]
use aloud_clipboard::{actions, Captured, Store};
use std::{sync::Arc, thread::sleep, time::Duration};

fn wait() {
    sleep(Duration::from_millis(2200));
}

fn previews(store: &Store) -> Vec<String> {
    store
        .list("", 50)
        .unwrap()
        .into_iter()
        .map(|e| e.preview)
        .collect()
}

#[cfg(target_os = "macos")]
fn copy_concealed(text: &str) {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{NSArray, NSString};
    let pb = NSPasteboard::generalPasteboard();
    let plain = NSString::from_str("public.utf8-plain-text");
    let marker = NSString::from_str("org.nspasteboard.ConcealedType");
    let types = NSArray::from_slice(&[&*plain, &*marker]);
    unsafe {
        pb.declareTypes_owner(&types, None);
        pb.setString_forType(&NSString::from_str(text), &plain);
        pb.setString_forType(&NSString::from_str(""), &marker);
    }
}

fn main() {
    let mut board = arboard::Clipboard::new().expect("clipboard");
    let saved = aloud_paste::Snapshot::capture(&mut board);

    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(dir.path()).unwrap());
    aloud_clipboard::watch::spawn(store.clone(), || true, || {});

    sleep(Duration::from_millis(800));
    board.set_text("live-test text from the user").unwrap();
    wait();
    assert!(
        store.list("", 10).unwrap().is_empty(),
        "history is off: nothing recorded"
    );

    store.set_enabled(true).unwrap();
    board.set_text("live-test alpha").unwrap();
    wait();
    assert_eq!(
        previews(&store),
        ["live-test alpha"],
        "a user copy is recorded"
    );

    aloud_paste::set_clipboard("live-test own write").unwrap();
    wait();
    assert_eq!(
        previews(&store).len(),
        1,
        "Aloud's own write is not history"
    );

    #[cfg(target_os = "macos")]
    {
        copy_concealed("live-test hunter2");
        wait();
        assert!(
            !previews(&store).iter().any(|p| p.contains("hunter2")),
            "a concealed copy is never stored"
        );
    }

    board
        .set_image(arboard::ImageData {
            width: 4,
            height: 4,
            bytes: vec![128u8; 64].into(),
        })
        .unwrap();
    wait();
    let list = store.list("", 10).unwrap();
    assert_eq!(list[0].preview, "Image · 4×4", "an image is recorded");

    actions::copy(&store, list[1].id).unwrap();
    wait();
    assert_eq!(
        board.get_text().unwrap(),
        "live-test alpha",
        "copy-back wrote the text"
    );
    assert_eq!(
        store.list("", 10).unwrap().len(),
        2,
        "copy-back did not add an entry"
    );

    if let Some(png) = std::env::args().nth(1) {
        store
            .record(Captured::Image {
                width: 1,
                height: 1,
                rgba: vec![0, 0, 0, 255],
            })
            .unwrap();
        // OCR reads the file directly, not through the store.
        let text = actions::run_ocr_for_example(std::path::Path::new(&png)).unwrap();
        println!("ocr: {text:?}");
    }

    saved.restore(&mut board).unwrap();
    println!("ok: watcher, own-write skip, concealed skip, image, copy-back (clipboard restored)");
}
