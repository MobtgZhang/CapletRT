# Kani Model-Checker Integration

This directory hosts the Kani harnesses that model-check the
`caplet-cap` capability model and the kernel's `Syscall` dispatch.

## Layout

* `harness/` — individual `#[kani::proof]` functions, one per
  verification target. Each file is a self-contained Rust module
  that can be model-checked in isolation.
* `run.sh` — convenience script that invokes `cargo kani` on every
  harness.
* `REUSE.md` — how the kernel's `unsafe` boundaries are tagged so
  the harnesses can be reused as living documentation.

## Running

Kani is not currently installed in this development environment.
When the toolchain is available:

```sh
cargo install --locked kani-verifier && cargo-kani setup
./tools/kani/run.sh
```

The script will:
1. Walk `harness/` and run `cargo kani --harness <file>` for each
   `.rs` file.
2. Emit a summary at the end with pass/fail counts.
3. Stop on the first failure (Kani's default) and propagate the
   exit code so CI can gate the build.

## Authoring a new harness

```rust
//! crates/caplet-cap/kani/<subject>.rs
//!
//! Property to prove: <one-line statement>.
//!
//! Covered branches: list of conditions over the inputs.
//! Assumptions: list of preconditions on the input shape.

#[kani::proof]
fn harness() {
    let n: u8 = kani::any();
    kani::assume(n < 64);
    let result = subject_function(n);
    assert!(result <= n as u16);
}
```

The harness must be `#[cfg(kani)]` so it does not affect the
regular `cargo build` / `cargo test` flow.
