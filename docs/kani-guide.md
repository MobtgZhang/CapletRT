# Kani Harness Guide

This is the practical guide for writing a proof in CapletRT. The current
catalogue, proof count, and module-to-property map live in
`docs/kani.md`; keep that file in sync when you add a harness.

## Where harnesses live

- Proof functions: `tools/kani/harness/*.rs`
- Harness crate: `tools/kani/src/lib.rs`
- Runner: `tools/kani/run_catalogue.sh`

The runner discovers every `#[kani::proof]`, including functions with extra
attributes such as `#[kani::unwind(16)]`, and executes them one at a time.
A missing `cargo-kani` exits `127` and is reported as `UNVERIFIED`; it is
never converted into a pass.

## Writing a proof

A harness must be small, self-contained, and state the property in its
module comment. Start from a real boundary in production code rather than a
reimplementation of it.

```rust
#![cfg(kani)]

#[kani::proof]
fn handle_rejects_zero_generation() {
    let index: u32 = kani::any();
    let generation: u32 = kani::any();
    kani::assume(index < 4096);
    kani::assume(generation == 0);

    let handle = Handle::<Dummy>::new(index, Generation(generation));
    assert!(!handle.is_valid());
}
```

Rules that keep the catalogue trustworthy:

- Bound inputs that would otherwise explode the state space, and make the
  bound part of the property you claim.
- Justify every `kani::assume`; an assumption that excludes the failing case
  turns a proof into a tautology.
- Never weaken or delete an assertion to make a run green. Fix the model or
  report the failure.
- Keep one property per harness so a failure names the broken contract.
- Use `#[kani::unwind(N)]` only when the loop bound is proven sufficient;
  document why `N` is enough.
- Harnesses that touch allocation, floating point, or inline assembly should
  be avoided or isolated; CBMC's model differs from the target.

## Running

```bash
cargo install --locked kani-verifier
cargo-kani setup

# One proof while iterating:
cargo kani -p caplet-kani-harnesses --harness attenuation_is_monotonic

# The full gate (serial, memory-heavy):
bash tools/kani/run_catalogue.sh
```

The full catalogue is CPU- and memory-intensive. Run it by itself rather
than alongside QEMU/Renode matrices or workspace builds.

## Review checklist

- The property is stated in prose and matches the assertions.
- The harness calls production code, not a copy.
- All assumptions are necessary and documented.
- A deliberate mutation of the implementation makes the proof fail.
- `docs/kani.md` lists the new proof and the total count is updated.

## Current limits

The catalogue is bounded model checking over selected security and state
machine boundaries. It does not prove the scheduler, MMU page tables,
interrupt latency, or the whole kernel. Do not present the catalogue as a
whole-system correctness proof.
