fn main() {
    // whisper.cpp's Metal code (ggml-metal-device.m) uses `@available()`
    // checks, which clang lowers to `___isPlatformVersionAtLeast` — a
    // compiler-rt builtin. Our release profile links with `-nodefaultlibs`
    // (a side effect of `panic = "abort"`), so compiler-rt is not pulled in
    // automatically and the symbol comes up undefined at link time.
    //
    // Link it explicitly on macOS. The path carries a clang major version, so
    // resolve it from the active clang rather than hardcoding it.
    #[cfg(target_os = "macos")]
    if let Some(dir) = clang_rt_dir() {
        println!("cargo:rustc-link-search=native={dir}");
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    }

    // Build stamp for the About row, so a bug report names the exact build.
    // Both fall back rather than fail: a release tarball has no `.git`, and a
    // build that dies because `git` is missing would be a worse outcome than
    // an unknown hash.
    //
    // The rerun rules are load-bearing, not hygiene. `tauri_build::build()`
    // emits its own, which cancels cargo's default "rerun on any change" — so
    // without these the stamp is whatever HEAD was the last time something in
    // `src-tauri` happened to change. That failed in the worst available way:
    // it reported a real, plausible, *wrong* commit rather than "unknown",
    // which is precisely what an About row exists to rule out.
    //
    // Both paths are needed. HEAD's own contents only change when the branch
    // does; committing on the same branch rewrites the ref file it points at.
    for path in git_stamp_deps() {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rustc-env=GIT_HASH={}", git_hash());
    println!(
        "cargo:rustc-env=BUILD_DATE={}",
        chrono::Utc::now().format("%Y-%m-%d")
    );

    tauri_build::build()
}

/// The files whose change means the stamp is stale: `HEAD`, and the ref it
/// points at. Resolved through `git` rather than assembled from `../..` so a
/// worktree — where `.git` is a file, not a directory — resolves correctly.
fn git_stamp_deps() -> Vec<String> {
    // The branch ref, when on one. Resolved through `symbolic-ref` rather than
    // `rev-parse --git-path @`, which yields a literal `.git/@` that does not
    // exist — and cargo treats a missing rerun path as *always* dirty, so that
    // spelling rebuilds the crate on every single `cargo build`.
    let head_ref = git(&["symbolic-ref", "-q", "HEAD"]);
    ["HEAD".to_string()]
        .into_iter()
        .chain(head_ref)
        .filter_map(|target| git(&["rev-parse", "--git-path", &target]))
        // A packed ref has no loose file, and a detached HEAD has no ref at
        // all. Both simply drop out: `HEAD` alone still catches a checkout,
        // and neither case is one a developer iterates in.
        .filter(|path| std::path::Path::new(path).exists())
        .collect()
}

/// `git` output, trimmed, or None when the command failed.
fn git(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())?;
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn git_hash() -> String {
    git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into())
}

/// The directory holding `libclang_rt.osx.a`, from `clang -print-runtime-dir`.
#[cfg(target_os = "macos")]
fn clang_rt_dir() -> Option<String> {
    let output = std::process::Command::new("clang")
        .arg("-print-runtime-dir")
        .output()
        .ok()?;
    let dir = String::from_utf8(output.stdout).ok()?.trim().to_string();
    std::path::Path::new(&dir)
        .join("libclang_rt.osx.a")
        .exists()
        .then_some(dir)
}
