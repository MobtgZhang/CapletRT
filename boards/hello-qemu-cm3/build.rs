//! Build script for the hello-qemu-cm3 board.
//!
//! ## Why this script provides `memory.x` and not `link.x`
//!
//! `cortex-m-rt` ships and installs its own `link.x`, which defines the
//! `.vector_table` section, sets `ENTRY(Reset)`, and `INCLUDE`s a
//! `memory.x` describing the board's memory layout. That script is what
//! creates the reset vector the CPU fetches from address 0 on boot.
//!
//! An earlier revision of this script copied a hand-written `link.x` into
//! `OUT_DIR`. Because `OUT_DIR` precedes the `cortex-m-rt` output
//! directory on the linker search path, that copy *shadowed*
//! `cortex-m-rt`'s script. The replacement defined no `.vector_table`, so
//! no reset vector was emitted and `Reset` was never defined. The
//! symptom was a link warning (`rust-lld: cannot find entry symbol`) and
//! a QEMU fatal `Lockup` dump with `R15=00000000` — the CPU had branched
//! into empty memory at address 0.
//!
//! The board's only real customisation is the memory map, so this script
//! writes just `memory.x` and lets `cortex-m-rt`'s `link.x` do the rest.

use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));

    // MPS2-AN385 (Cortex-M3) memory map:
    //   code/vector table : 4 MiB at 0x0000_0000 (ZBT SSRAM1)
    //   data              : 4 MiB at 0x2000_0000 (ZBT SSRAM2+3)
    //
    // IMPORTANT: `cortex-m-rt`'s link.x does a bare `INCLUDE memory.x` at
    // file scope (it does NOT wrap it in a MEMORY block of its own). This
    // file must therefore contain the complete `MEMORY { ... }` block.
    // Emitting only bare region lines fails with
    // `memory.x:2: unknown directive: FLASH`.
    let memory_x = "\
MEMORY
{
  FLASH (rx) : ORIGIN = 0x00000000, LENGTH = 4M
  RAM  (rwx) : ORIGIN = 0x20000000, LENGTH = 4M
}
";

    let memory_x_path = out.join("memory.x");
    std::fs::write(&memory_x_path, memory_x).expect("failed to write memory.x");

    // Put OUT_DIR on the linker search path so `INCLUDE memory.x` resolves
    // to the file written above, and so `-Tlink.x` finds cortex-m-rt's
    // script (its output dir is also on the path).
    println!("cargo:rustc-link-search={}", out.display());

    // Tell the linker to use cortex-m-rt's link.x.
    //
    // This is required: `cortex-m-rt` ships `link.x` and its build script
    // exports it, but it does NOT pass `-Tlink.x` itself — the board must.
    // Without this line the ELF is linked with the default script, which
    // has no `.vector_table`/`.text` for our regions; every section ends
    // up at address 0, `ENTRY(Reset)` is never seen, and rust-lld warns
    // `cannot find entry symbol _start` (the *default* ELF entry) before
    // producing an empty image.
    println!("cargo:rustc-link-arg=-Tlink.x");

    println!("cargo:rerun-if-changed=build.rs");
}
