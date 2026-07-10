# ADR-004: Resource Budgets (Memory, CPU, Message) Per Task

Status: Accepted
Date: 2026-07-09
Supersedes: —

## Context

Industrial deployments must guarantee that a misbehaving task
cannot exhaust RAM, starve the CPU, or flood the IPC bus. The
capability model guarantees that a task can only touch the
handles it has been *given*, but it does not bound *how much*
of a resource the task can consume.

FreeRTOS provides the same idea with the "vTaskDelay until"
pattern, but most implementations only bound CPU time (via
time-slicing) and leave memory and IPC budgets to the
application.

## Decision

Each task carries a `Budget<T>` for each of three resource
classes:

1. **Memory** — checked at allocation time against the task's
   static heap quota.
2. **CPU** — incremented every tick the task is in the Running
   state; tasks that exceed their window are demoted for one
   window before being allowed back in.
3. **Message** — a counting semaphore per task decremented on
   every IPC send.

When a budget is exhausted the task receives a
`BudgetExceeded` event on its event channel and is moved to
Blocked until the budget is replenished (typically by an
external supervisor or a periodic refill).

## Consequences

- Resource exhaustion is handled gracefully, not via panic.
- Auditing is centralised: every `BudgetEvent` lands in the
  audit log (see `caplet-cap::audit`).
- We need a kernel-side counter for CPU usage; this is cheap
  on Cortex-M because the running task is a single global
  variable.

## Alternatives considered

- **Hard limits via MPU**: rejected — MPU regions are scarce
  on Cortex-M0 (8 regions); we want a portable budget system.
- **Application-level quotas**: rejected — easy to bypass by
  accident; we need the kernel to be the gatekeeper.