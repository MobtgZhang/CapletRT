//! Make the board linker script available regardless of Cargo's CWD.

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
