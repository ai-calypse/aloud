// Verifies installed_apps() against real bundles on this machine.
fn main() {
    let apps = aloud_selection::installed_apps();
    println!("{} apps:", apps.len());
    for a in &apps {
        let icon = a
            .icon
            .as_deref()
            .map(|s| format!("{}…", &s[..s.len().min(40)]));
        println!(
            "  {:<28} icon: {}",
            a.name,
            icon.unwrap_or_else(|| "—".into())
        );
    }
    assert!(!apps.is_empty(), "expected at least one installed app");
    let with_icons = apps.iter().filter(|a| a.icon.is_some()).count();
    println!("\n{}/{} have icons", with_icons, apps.len());
    assert!(with_icons > 0, "expected at least one icon to extract");
}
