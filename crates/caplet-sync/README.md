# `caplet-sync`

Synchronisation primitives for single-core Cortex-M and
multi-core test environments.

## Modules

| Module         | Purpose                                       |
|----------------|-----------------------------------------------|
| `spin`         | `SpinLock<T>` — short critical sections.       |
| `mutex`        | Blocking `Mutex<T>` (uses the scheduler).     |
| `semaphore`    | Counting `Semaphore`.                         |
| `channel`      | Async MPMC channel.                           |
| `sync_channel` | Synchronous rendezvous channel.               |
| `event`        | `Event` — single-bit one-shot signal.         |

## Conventions

- All primitives are `no_std`.
- `SpinLock::lock()` spins with `core::hint::spin_loop()`;
  callers are expected to hold the lock for short periods.
- Blocking primitives assume a single-core target and use
  the kernel's scheduler.

## Status

Stable.