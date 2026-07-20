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
    // ponytail: tauri-build emits its own rerun-if-changed rules, which stop
    // cargo from re-running this on every source change — so the hash can lag
    // a commit until something in `src-tauri` forces a rebuild. `touch
    // build.rs` is the workaround; a `.git/HEAD` rerun rule is the fix.
    println!("cargo:rustc-env=GIT_HASH={}", git_hash());
    println!(
        "cargo:rustc-env=BUILD_DATE={}",
        chrono::Utc::now().format("%Y-%m-%d")
    );

    tauri_build::build()
}

fn git_hash() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map_or_else(|| "unknown".into(), |hash| hash.trim().to_string())
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
