// Verifies installed_apps() against real bundles on this machine.
fn main() {
    // Cold vs warm: the scan is cached for the process lifetime, so the second
    // call must not pay for the ~70 `sips` spawns again.
    let cold_start = std::time::Instant::now();
    let apps = aloud_selection::installed_apps();
    let cold = cold_start.elapsed();

    let warm_start = std::time::Instant::now();
    let again = aloud_selection::installed_apps();
    let warm = warm_start.elapsed();

    assert_eq!(apps.len(), again.len(), "cached scan changed under us");
    println!("cold: {cold:?}   warm: {warm:?}\n");
    assert!(
        warm < cold / 10,
        "warm call ({warm:?}) should be far cheaper than cold ({cold:?}) — cache not working"
    );

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
