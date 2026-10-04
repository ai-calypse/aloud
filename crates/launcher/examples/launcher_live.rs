// Runs the real `shortcuts`, `mdfind` and `open` paths; opens nothing.
//   cargo run -p aloud-launcher --example launcher_live [file-name-fragment]
fn main() {
    let shortcuts = aloud_launcher::list_shortcuts().expect("shortcuts list");
    println!(
        "shortcuts: {} {:?}",
        shortcuts.len(),
        &shortcuts[..shortcuts.len().min(5)]
    );

    let query = std::env::args().nth(1).unwrap_or_else(|| "readme".into());
    let found = aloud_launcher::search_files(&query, 5).expect("search");
    for f in &found {
        println!("search {:?}: {} ({})", query, f.name, f.folder);
    }
    assert!(found
        .iter()
        .all(|f| std::path::Path::new(&f.path).is_file()));

    let recent = aloud_launcher::recent_files(5).expect("recent");
    for f in &recent {
        println!("recent: {} ({})", f.name, f.folder);
    }
    assert!(aloud_launcher::search_files("-h", 5).unwrap().is_empty());
    assert!(aloud_launcher::open_app("No Such App Aloud").is_err());
    println!("ok");
}
