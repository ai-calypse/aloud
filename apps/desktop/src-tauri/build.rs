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

    tauri_build::build()
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
