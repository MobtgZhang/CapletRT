//! Build script for the hello-qemu-cm4 board.
//!
//! ## What this script does and does not do
//!
//! `cortex-m-rt` ships the real `link.x` (it defines `.vector_table`, sets
//! `ENTRY(Reset)` and `INCLUDE`s `memory.x`). This script:
//!
//!   * copies the board's `memory.x` into `OUT_DIR`, where
//!     `cortex-m-rt`'s `INCLUDE memory.x` will find it;
//!   * copies `linkme.x` into `OUT_DIR` so link-time task registrations
//!     are placed in flash rather than in the uncopied RAM orphan section;
//!   * puts `OUT_DIR` on the linker search path;
//!   * passes `-Tlink.x -Tlinkme.x` so the linker uses those scripts.
//!
//! It deliberately does NOT write its own `link.x`. An earlier revision
//! copied a hand-written `link.x` into `OUT_DIR`; since `OUT_DIR` precedes
//! `cortex-m-rt`'s output directory on the search path, that copy
//! shadowed `cortex-m-rt`'s script. The replacement had no
//! `.vector_table`, so no reset vector was emitted, `Reset` was undefined,
//! and the resulting image stalled with `R15=00000000` (QEMU fatal
//! `Lockup`).

use std::path::PathBuf;

fn main() {
    let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));

    // Ship the board memory map to OUT_DIR for cortex-m-rt's link.x.
    let mem_src = project_dir.join("memory.x");
    let mem_out = out.join("memory.x");
    std::fs::copy(&mem_src, &mem_out).expect("failed to copy memory.x");

    let linkme_src = project_dir.join("linkme.x");
    let linkme_out = out.join("linkme.x");
    std::fs::copy(&linkme_src, &linkme_out).expect("failed to copy linkme.x");

    println!("cargo:rustc-link-search={}", out.display());

    // `cortex-m-rt` does not pass `-Tlink.x` itself; the board must.
    println!("cargo:rustc-link-arg=-Tlink.x");
    println!("cargo:rustc-link-arg=-Tlinkme.x");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=linkme.x");
}
