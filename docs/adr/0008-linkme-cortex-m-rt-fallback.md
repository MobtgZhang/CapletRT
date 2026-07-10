# ADR-008: Fallback Strategy When `linkme` Conflicts With `cortex-m-rt`

Status: Accepted
Date: 2026-07-09
Supersedes: —

## Context

ADR-0005 (the macro plan for `#[caplet::task]`) assumed we
could use `linkme` to register task handlers at link time.
`linkme` works by emitting into a custom linker section that
the host linker then concatenates. On Cortex-M with
`cortex-m-rt` 0.7 we observed the custom section is placed
before the `.text` vector table and the boot ROM cannot find
the reset handler.

## Decision

When this incompatibility surfaces, the **fallback** is:

1. Disable `linkme` in `caplet-macros`.
2. Have the `#[caplet::task]` macro emit a `#[used]`
   `static` per task so the linker keeps the symbol even
   without a custom section.
3. `caplet-macros-exports` builds a `TaskTable` by walking
   the `static`s directly (see `caplet_task::task_table`).

If the fallback also breaks (e.g. when moving to AArch64
where `#[used]` has different semantics), the second-tier
fallback is to manually register each task in `kernel_init`
with no codegen tricks.

## Consequences

- `linkme` is opt-in, not default. The QEMU demo boots
  without it.
- The task table size grows linearly with the number of
  `#[caplet::task]` invocations, which is fine for the
  M2 demo (~10 tasks).
- The boot sequence is identical to a hand-rolled
  FreeRTOS-style table, which keeps the verifier happy.

## Alternatives considered

- **Patch `linkme`**: rejected — the bug is in the
  interaction with `cortex-m-rt`; we would be maintaining
  an out-of-tree fork.
- **Drop `#[caplet::task]` and require hand-rolled
  `kernel_init` calls**: rejected — defeats the purpose of
  the macro and forces users to memorise the table shape.