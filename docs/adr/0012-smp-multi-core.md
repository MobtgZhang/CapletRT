# ADR-0012: SMP Multi-Core Bring-Up (M3)

Status: Accepted
Date: 2026-07-10
Supersedes: —

## Context

docv1.md §10.4 committed M3 to "multi-core scheduling across
up to 4 cores on the QEMU vexpress-a9 board", with a single
`Arch::request_reschedule` (IPI for Cortex-A, SVC for
Cortex-M) as the inter-processor interrupt primitive. The
machinery was added incrementally: `smp.rs` carries the
multi-CPU bookkeeping, `smp_scheduler.rs` owns per-CPU run
queues, and `cpu.rs` records `MAX_CPUS` / `CpuId` constants.

## Decision

- `CPU_COUNT = 4` (Cortex-A9) / `CPU_COUNT = 1` (all
  Cortex-M). `MAX_CPUS = 4` so the binary layout is fixed.
- `SmpScheduler` keeps one run queue per CPU plus a shared
  pool of "ready-to-migrate" tasks. Load balancing is
  opportunistic: at every `pick_next` call the sched checks
  the sibling queues and steals a task if any of them is at
  least 2 entries longer.
- IPI delivery uses GIC SGIs (SGI 0..3) on Cortex-A and the
  existing `Arch::request_reschedule` on Cortex-M. The IDs
  match `ports/cortex-a/src/gic.rs::IPI_*`.
- `Arch::max_cpus()` is the single source of truth: callers
  that want a board-specific limit must specialize via
  `cfgs`, not runtime patching.

## Consequences

- The default loader (`boards/hello-qemu-cm4`) still uses a
  single core; the multi-core code is exercised only on the
  Cortex-A9 port and on the host `smp_smoke.rs` integration
  test.
- Tasks may be unpinned (`AffinityMask::ANY`) or pinned to one or more CPUs
  with `AffinityMask`. `SmpScheduler::migrate` rejects a destination excluded
  by the task's mask (`MigrateError::AffinityDenied`), and the load balancer
  skips candidates whose mask does not admit the destination. It never
  rewrites an affinity mask to make a migration succeed.
- Unpinned tasks can move at an activation/`pick_next` boundary. The QEMU
  Cortex-A9 SMP smoke test exercises secondary-core bring-up and per-core
  workers at `-smp 2` and `-smp 4`.
- The RISC-V port is single-hart today. Future RISC-V SMP work can reuse the
  architecture-independent `SmpScheduler`; the RISC-V `Arch` implementation
  still needs the equivalent IPI/startup path.
