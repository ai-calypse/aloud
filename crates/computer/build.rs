//! Compiles the vendored MacosUseSDK helpers, and tells the crate where they
//! landed.
//!
//! Building Swift from `build.rs` is not free — it is ~25s cold — but the
//! alternative is a binary committed to the repo or a network clone at build
//! time, and both are worse. Warm builds are a no-op: Swift Package Manager
//! does its own change detection, and the rerun-if directives below mean cargo
//! only asks when the vendored source actually changes.
//!
//! A failure here is *not* a build failure. A machine without Swift still
//! builds the app; `helper::locate` finds nothing, `tools()` returns empty, and
//! computer-use is simply absent — the same way an empty Shortcuts library
//! yields no shortcut tool.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Consumed by `helper::locate` for the un-stripped sidecar name.
    println!(
        "cargo:rustc-env=HELPER_TARGET_TRIPLE={}",
        std::env::var("TARGET").unwrap_or_default()
    );

    let sdk = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/MacosUseSDK");
    println!("cargo:rerun-if-changed={}/Sources", sdk.display());
    println!("cargo:rerun-if-changed={}/Package.swift", sdk.display());

    // Out of OUT_DIR: one build per profile, cleaned by `cargo clean`, and never
    // written into the vendored source tree.
    let build_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("swift");

    // Declared before the early return and any failure, because `helper.rs`
    // needs the constant to exist even when nothing was built — it just finds
    // no file there and reports the capability as absent.
    let bin_dir = bin_path(&sdk, &build_dir).unwrap_or_else(|| build_dir.join("release"));
    println!("cargo:rustc-env=HELPER_BUILD_DIR={}", bin_dir.display());

    if std::env::var("TARGET").is_ok_and(|t| !t.contains("apple-darwin")) || !sdk.is_dir() {
        return;
    }

    // `--product` is singular — passing it twice builds only the last one, which
    // silently shipped an input helper with no traversal helper. One invocation
    // per product.
    for product in ["TraversalTool", "InputControllerTool"] {
        let built = Command::new("swift")
            .current_dir(&sdk)
            .args(["build", "-c", "release", "--scratch-path"])
            .arg(&build_dir)
            .args(["--product", product])
            .status();

        match built {
            Ok(status) if status.success() => {}
            // Warned, not failed: the app must still build on a machine without
            // a Swift toolchain. The capability disappears; the build does not.
            Ok(status) => println!(
                "cargo:warning={product} failed to build ({status}); screen control will be unavailable"
            ),
            Err(e) => {
                println!("cargo:warning=swift not found ({e}); screen control will be unavailable");
                return;
            }
        }
    }
}

/// Where SwiftPM puts the built binaries. Asked rather than assumed: the scratch
/// path gains an arch directory (`arm64-apple-macosx/release`) that is not
/// documented and would differ on an Intel machine.
fn bin_path(sdk: &PathBuf, build_dir: &PathBuf) -> Option<PathBuf> {
    let out = Command::new("swift")
        .current_dir(sdk)
        .args(["build", "-c", "release", "--scratch-path"])
        .arg(build_dir)
        .arg("--show-bin-path")
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}
