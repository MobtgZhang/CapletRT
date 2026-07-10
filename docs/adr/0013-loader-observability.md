# ADR-0013: Loader Observability — Atomic Counters + Optional Audit Ring (M3)

Status: Accepted
Date: 2026-07-10
Supersedes: —

## Context

Once the loader is accepting third-party `.capp` files, an
operator has to be able to answer two questions quickly:

1. "Is the loader currently succeeding or failing?" — a
   pair of integer counters is sufficient.
2. "Which modules just failed, and what was the failure kind?"
   — that needs a ring buffer of `(name, key_id, outcome)`.

docv1.md §14.2 called for "small-step ingest telemetry" but
did not specify the contract. M3 ships `caplet_loader::metrics`,
which addresses both questions with two pieces:

- `LoaderMetrics` (atomic counters) — always on.
- `AuditRing` (16-entry bounded ring) — gated behind the
  `metrics_ring` feature so embedding boards can opt out.

## Decision

- `LoaderMetrics` uses `core::sync::atomic::AtomicU64` and
  exposes only `Relaxed` ordering. Operators read it via
  `snapshot()`; the snapshot is best-effort but never
  unsafe.
- The `AuditRing` is a fixed `[Option<AuditRecord>; 16]`
  behind a `caplet_sync::spin::SpinLock`. Total cost is
  16 × 32 bytes ≈ 512 bytes of `.bss`.
- The host-only `hello-dynamic-loader` demo reads
  `format_snapshot(metrics.snapshot())` once on each run and
  prints the line so a developer can sanity-check the gate.

## Consequences

- Production Cortex-M builds with no allocator and no
  dynamic memory still get atomic-counter-level observability.
- The optional ring buffer requires a SpinLock and
  `caplet-sync`; this is acceptable on Cortex-A and on the
  larger Cortex-M boards (CM4-F446ZE has 128 KiB of SRAM).
- Future work (M4, "remote ingest") can build a no-op
  implementation of `push_outcome` if the `metrics_ring`
  feature is disabled, so the contract is closed.
