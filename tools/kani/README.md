# Kani Model-Checker Integration

This directory hosts the Kani harnesses for capability derivation, loader
header validation, MMU permissions, SMP/IPC, syscall dispatch, and the
network state machines.

## Layout

- `harness/` contains the `#[kani::proof]` functions, grouped by subject.
- `src/lib.rs` registers each harness module under `cfg(kani)`.
- `run.sh` runs the whole `caplet-kani-harnesses` package in one `cargo kani`
  invocation.
- `run_catalogue.sh` extracts each proof function and runs it independently,
  including proofs annotated with `#[kani::unwind(N)]`.
- `REUSE.md` documents how proofs are shared as living specifications.

The harness package builds `caplet-kernel` without its optional
`module-loader` feature. The loader pulls large cryptographic implementations
into every syscall-dispatch call graph; its parser is instead covered by
`loader_manifest.rs`, and its end-to-end behaviour by the host loader tests.

## Running

```bash
cargo install --locked kani-verifier
cargo-kani setup

# One named proof:
cargo kani -p caplet-kani-harnesses --harness attenuation_is_monotonic

# Hard-gated full catalogue:
bash tools/kani/run_catalogue.sh

# Override the default 8 GiB per-run address-space guard only when a proof has
# been reviewed and genuinely needs more memory:
KANI_MEMORY_LIMIT_KIB=12582912 bash tools/kani/run_catalogue.sh
```

The catalogue runner applies a per-process memory limit so an unbounded
symbolic path cannot exhaust the host. A proof hitting the limit is reported as
a failure, never as a pass. Keep symbolic inputs out of broad dispatchers when
concrete, table-driven coverage can express the same property.

If `cargo-kani` is not on `PATH`, both scripts print `UNVERIFIED` and exit
non-zero. A missing verifier, a timeout, or a failed build is never reported as
a passing proof.

## Authoring a proof

Add a module under `harness/` (and register it in `src/lib.rs` when new), then
write a self-contained property:

```rust
#[kani::proof]
#[kani::unwind(16)]
fn property_name() {
    let input: u8 = kani::any();
    kani::assume(input < 64);
    assert!(subject_function(input) <= input);
}
```

The harness file must be gated with `#![cfg(kani)]` so normal builds and host
tests are unaffected. Prefer asserting a real postcondition over calling the
subject without a claim. Do not use `kani::assume` to hide a reachable failure,
and do not remove assertions to make a proof green.
