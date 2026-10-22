//! Benchmark: loader throughput (M3).
//!
//! Measures how many `parse_capp` + `verify_signature` cycles the
//! host can do per second when handed a tiny in-memory `.capp`
//! payload. Use for tracking regressions on the parse path; the
//! authoritative target is "small Cortex-M load (16 KiB image)
//! completes in < 50 ms on the board", which can't be measured
//! directly on a hosted runner.

use caplet_bench_harness::{report, time};
use caplet_loader::capp::{HEADER_SIZE, MAGIC, SIGNATURE_SIZE, SUPPORTED_VERSION};
use caplet_loader::parse_capp;

/// Build a tiny CAPP byte stream. Same layout as
/// `boards/hello-dynamic-loader/src/main.rs::build_sample_capp`
/// but kept here so the bench is self-contained.
fn build_small_capp() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&SUPPORTED_VERSION.to_le_bytes());
    out.extend_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&0x6000_0000u32.to_le_bytes()); // entry
    out.extend_from_slice(&4096u32.to_le_bytes()); // stack
    out.push(8); // priority
    out.push(0); // flags (no signature)
    out.extend_from_slice(&[0u8; 2]); // pad
    let text_off = HEADER_SIZE;
    out.extend_from_slice(&(text_off as u32).to_le_bytes());
    out.extend_from_slice(&4u32.to_le_bytes()); // text_size
    let data_off = text_off + 4;
    out.extend_from_slice(&(data_off as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // data_size
    out.extend_from_slice(&(data_off as u32).to_le_bytes()); // reloc_off
    out.extend_from_slice(&0u32.to_le_bytes()); // reloc_count
    out.extend_from_slice(&(data_off as u32).to_le_bytes()); // cap_off
    out.extend_from_slice(&1u32.to_le_bytes()); // cap_count
    let name_off = data_off + 32;
    out.extend_from_slice(&(name_off as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; SIGNATURE_SIZE]);
    out.extend_from_slice(b"HEL\x00");
    out.extend_from_slice(b"log\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00");
    out.extend_from_slice(&0xFFu32.to_le_bytes());
    out.extend_from_slice(b"hello-bench\x00");
    out
}

fn main() {
    let bytes = build_small_capp();
    println!("loader_throughput using {} bytes", bytes.len());

    let ns = time(50_000, || {
        let _ = parse_capp(&bytes).expect("parse");
    });
    report!("loader_throughput", ns);
}
