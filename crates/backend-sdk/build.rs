//! Build script for the Sony SDK backend.
//!
//! Sony's licence does not allow us to commit their headers or libraries, so the
//! build looks for them in a directory named by `SONY_SDK_ROOT`. When the
//! variable is missing we do not fail: we compile a stub backend that reports
//! "SDK not configured". That keeps `cargo test` and UI work possible before
//! approval lands. Release builds must set the variable; the stub must not ship.

use std::env;
use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(sony_sdk)");
    println!("cargo:rerun-if-env-changed=SONY_SDK_ROOT");
    println!("cargo:rerun-if-changed=native");

    match env::var("SONY_SDK_ROOT") {
        Ok(root) if !root.trim().is_empty() => configure(&root),
        _ => println!(
            "cargo:warning=SONY_SDK_ROOT is not set: building the stub backend, which reports \
             'SDK not configured'. Set SONY_SDK_ROOT to the extracted Sony Camera Remote SDK \
             before building a release."
        ),
    }
}

fn configure(root: &str) {
    let root = Path::new(root);

    if !root.is_dir() {
        panic!(
            "SONY_SDK_ROOT is set to '{}', which is not a directory",
            root.display()
        );
    }

    // The exact layout is fixed in milestone M1, once the real SDK is in hand.
    // Until then we only check the directory exists and record the path.
    println!(
        "cargo:warning=Building against the Sony SDK at {} (binding lands in M1)",
        root.display()
    );
    println!("cargo:rustc-cfg=sony_sdk");
    println!("cargo:rustc-env=SONY_SDK_ROOT={}", root.display());
}
