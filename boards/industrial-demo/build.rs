//! Build script for the industrial-demo board.

use std::path::PathBuf;

fn main() {
    let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = project_dir.join("memory.x");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let _ = std::fs::copy(&src, out.join("memory.x"));
    println!("cargo:rustc-link-search={}", out.display());

    let build_dir = out.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf());
    if let Some(build_dir) = build_dir {
        if let Ok(entries) = std::fs::read_dir(&build_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().starts_with("cortex-m-rt-") {
                    let target = entry.path().join("out").join("memory.x");
                    let _ = std::fs::create_dir_all(entry.path().join("out"));
                    let _ = std::fs::copy(&src, &target);
                }
            }
        }
    }
    println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=memory.x");
}