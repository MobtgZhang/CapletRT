//! Build script for the hello-nucleo-f446ze-qemu board.
//!
//! STM32F446ZE (Cortex-M4F) on QEMU.
//!
//! ## What this script does
//!
//!   * copies the board's `memory.x` into `OUT_DIR`, where
//!     `cortex-m-rt`'s `link.x` `INCLUDE`s it;
//!   * puts `OUT_DIR` on the linker search path;
//!   * passes `-Tlink.x` so the linker uses `cortex-m-rt`'s script.
//!
//! ## What it deliberately does NOT do
//!
//! It does not copy a hand-written `link.x` into `OUT_DIR`. An earlier
//! revision did; because `OUT_DIR` precedes `cortex-m-rt`'s output
//! directory on the search path, that copy shadowed `cortex-m-rt`'s
//! script. The replacement defined no `.vector_table`, so no reset vector
//! was emitted and the image stalled at `R15=00000000` (QEMU fatal
//! `Lockup`).

use std::path::PathBuf;

fn main() {
    let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));

    // Ship the board memory map for cortex-m-rt's `INCLUDE memory.x`.
    let memory_src = project_dir.join("memory.x");
    let memory_out = out.join("memory.x");
    std::fs::copy(&memory_src, &memory_out).expect("failed to copy memory.x");

    println!("cargo:rustc-link-search={}", out.display());

    // `cortex-m-rt` does not add `-Tlink.x` itself; the board must.
    println!("cargo:rustc-link-arg=-Tlink.x");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=memory.x");
}
