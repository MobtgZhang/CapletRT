//! Build script for the industrial-demo board.
//!
//! Cortex-M builds use `cortex-m-rt`'s `link.x` plus `memory.x`.
//! RISC-V builds use the board's own `link.x`, which places the image
//! at the QEMU-virt / Renode DRAM base.

use std::path::PathBuf;

fn main() {
    let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    if arch == "arm" {
        let src = project_dir.join("memory.x");
        let _ = std::fs::copy(&src, out.join("memory.x"));

        // `cortex-m-rt` resolves `memory.x` from its own OUT_DIR; mirror
        // the board layout there as well.
        let build_dir = out
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
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
    } else {
        let src = project_dir.join("link.x");
        std::fs::copy(&src, out.join("link.x")).expect("failed to copy link.x");
    }

    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=link.x");
}
