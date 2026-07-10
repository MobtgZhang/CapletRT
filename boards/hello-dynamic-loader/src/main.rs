//! Hello-dynamic-loader demo for M3.
//!
//! This is a host-target binary (run with `cargo run`) that:
//! 1. Builds a `.capp` byte stream in memory (mirroring what
//!    `tools/elf2capp.py` would produce for an embedded target).
//! 2. Calls `caplet_loader::load_module` to parse and stage the
//!    module in the host-side `ModulePool`.
//! 3. Prints the manifest and the granted capability set so the
//!    developer can verify that the loader accepted the module.
//!
//! The intent is to exercise the loader's API on a machine that
//! has no MMU and no real UART, so the static and dynamic modes
//! can be compared back-to-back on a developer laptop.

use caplet_loader::allocator::ModulePool;
use caplet_loader::manifest::ModuleFormat;
use caplet_loader::parse_capp;
use caplet_loader::sandbox::{granted_caps_summary, load_module, manifest_to_string};

use caplet_cap::CSpace;

/// Build a minimal `.capp` byte stream with one text section, no
/// relocs, and a single granted cap tagged "log".
fn build_sample_capp() -> Vec<u8> {
    use caplet_loader::capp::{
        HEADER_SIZE, MAGIC, SIGNATURE_SIZE, SUPPORTED_VERSION,
    };
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&SUPPORTED_VERSION.to_le_bytes());
    out.extend_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&0x6000_0000u32.to_le_bytes()); // entry
    out.extend_from_slice(&4096u32.to_le_bytes()); // stack
    out.push(8); // priority
    out.push(0); // flags (no signature required for the demo)
    out.extend_from_slice(&[0u8; 2]); // pad
    // text @ HEADER_SIZE
    let text_off = HEADER_SIZE;
    out.extend_from_slice(&(text_off as u32).to_le_bytes());
    out.extend_from_slice(&4u32.to_le_bytes()); // text_size
    let data_off = text_off + 4;
    out.extend_from_slice(&(data_off as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // data_size
    // relocs @ data_off, 0
    out.extend_from_slice(&(data_off as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    // caps @ data_off, 1
    out.extend_from_slice(&(data_off as u32).to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    // name @ data_off + 32
    let name_off = data_off + 32;
    out.extend_from_slice(&(name_off as u32).to_le_bytes());
    // signature (64 bytes zero) at bytes[56..120]
    out.extend_from_slice(&[0u8; SIGNATURE_SIZE]);
    // text payload (4 bytes)
    out.extend_from_slice(b"HEL\x00");
    // cap descriptor: 28-byte tag "log" + 4-byte perms
    out.extend_from_slice(b"log\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00");
    out.extend_from_slice(&0xFFu32.to_le_bytes());
    // name
    out.extend_from_slice(b"hello-dynamic\x00");
    out
}

fn main() {
    println!("hello-dynamic-loader: building a .capp in memory");
    let bytes = build_sample_capp();

    println!("parsing container format");
    let parsed = match parse_capp(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("parse_capp failed: {e:?}");
            std::process::exit(1);
        }
    };
    if !matches!(parsed.manifest.format, ModuleFormat::Capp) {
        eprintln!("expected Capp, got {:?}", parsed.manifest.format);
        std::process::exit(1);
    }
    println!("{}", manifest_to_string(&parsed.manifest));

    println!("loading module into pool");
    let mut pool = ModulePool::new(0x6000_0000, 16 * 1024 * 1024);
    let cspace = CSpace::new();
    let loaded = match load_module(&bytes, &mut pool, &cspace) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("load_module failed: {e:?}");
            std::process::exit(1);
        }
    };
    println!("loaded module: entry=0x{:x} base=0x{:x} size={}",
        loaded.manifest.entry_point, loaded.base_vaddr, loaded.image_size);

    let summary = granted_caps_summary(&loaded.manifest);
    println!("granted caps:");
    for (tag, perms) in &summary {
        println!("  - {tag}: 0x{perms:x}");
    }
    println!("hello-dynamic-loader: done");
}
