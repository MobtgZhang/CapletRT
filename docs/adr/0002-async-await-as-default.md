# ADR-002: Async/Await as the Default Concurrency Primitive

Status: Accepted
Date: 2026-07-09
Supersedes: —

## Context

A Cortex-M class CPU cannot afford a stack per thread. We must
support many concurrent tasks without pre-allocating a stack
frame for every possible blocking syscall.

Two viable patterns exist:

1. **Stackful coroutines** (e.g. Boost.Coroutine, Rust's
   `corosensei`). Each task owns a heap-allocated stack;
   context switching copies the stack pointer.
2. **Stackless async/await** (the Rust standard model). Each
   task owns a `Future` state machine; the compiler splits it
   across its `await` points so the maximum live stack is just
   the size of one state.

## Decision

CapletRT's default is **stackless async/await**, implemented
by `caplet-async`. Tasks are spawned as `Future`s; blocking
syscalls (IPC, FS, socket) return a `Future` that resolves when
the kernel wakes the task via its `Waker`. The
`#[caplet::task]` macro hides the boilerplate.

Stackful coroutines and preemptive threads remain optional
escape hatches for legacy code, but new tasks should use
`async`.

## Consequences

- Memory cost per task is `sizeof(state machine)` plus one
  stack frame; we can support thousands of tasks on a 256 KiB
  MCU.
- The compiler enforces that any state held across `await`
  is `Send + Sync`; this gives us a static check that our
  scheduler can safely poll the future.
- Borrow-checker-safe cross-task references (`caplet-task`'s
  `task_ref`) are mandatory because the state is shared.

## Alternatives considered

- **One stack per task (FreeRTOS)**: rejected — 1 KiB × N
  tasks does not scale on Cortex-M0 with 8 KiB RAM.
- **Pure cooperative green threads**: rejected — starves
  priority inversions. We need the kernel to choose which
  ready task runs.