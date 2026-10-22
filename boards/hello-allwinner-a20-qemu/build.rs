//! Build script for the hello-allwinner-a20-qemu board.
//!
//! Allwinner A20 (Cortex-A8 / ARMv7-A) on QEMU `vexpress-a9`.
//!
//! This board does **not** use `cortex-m-rt`: it is an A-profile target, so
//! it ships its own `link.x` and its own reset stub. This script makes that
//! script reachable by the linker regardless of the current working
//! directory.
//!
//! ## Why a build script is needed
//!
//! The board passes `-Tlink.x`, and the linker resolves a bare `-Tlink.x`
//! relative to the *current working directory*. Cargo may be invoked from
//! the workspace root (`cargo build -p <board>`), where no `link.x`
//! exists, producing `rust-lld: error: cannot find linker script link.x`.
//!
//! Copying `link.x` into `OUT_DIR` and putting `OUT_DIR` on the search path
//! makes the script resolve deterministically from any working directory.
//! (Relying on the board's own directory only worked when the build
//! happened to be launched from inside it.)

use std::path::PathBuf;

fn main() {
    let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));

    let link_src = project_dir.join("link.x");
    let link_out = out.join("link.x");
    std::fs::copy(&link_src, &link_out).expect("failed to copy link.x");

    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-arg=-Tlink.x");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=link.x");
}
