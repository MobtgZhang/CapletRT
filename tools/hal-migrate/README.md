# `hal-migrate` (M3 placeholder)

This directory is a **placeholder** for the HAL v0.1 -> v0.2 migration
helper described in the M2 plan (item 2.2.2). It intentionally contains no
code-rewriting implementation yet.

## Status

- The crate compiles and exposes the migration checklist as data.
- It does **not** parse, rewrite, or validate user board sources.
- It is excluded from the root workspace and from CI gates.

## Intended v0.1 -> v0.2 flow (M3)

1. Read `caplet-hal`'s `HAL_VERSION_MAJOR/MINOR/PATCH` from the target
   workspace and refuse to migrate when the source major is not `0.1`.
2. Locate board crates by their dependency on `caplet-hal = "0.1"`.
3. Apply the documented trait migrations (see `docs/porting.md` and
   `crates/caplet-hal/README.md`):
   - `Uart::write` error type unification,
   - `Phy` link-state reporting,
   - `Timer` tick-source split,
   - capability-aware `Interrupt` registration.
4. Rewrite `Cargo.toml` requirements to `caplet-hal = "0.2"`.
5. Run `cargo check` and `tools/hal_version_gate.sh` on the result.
6. Emit a diff summary and leave the tree untouched on any failure.

Until step 3 has a reviewed implementation, run the manual procedure in
`docs/porting.md` and let `tools/hal_version_gate.sh` enforce the version
bump.
