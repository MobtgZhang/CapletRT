# `caplet-async`

The async executor and `Waker` plumbing. Implements a single-threaded
executor with a real `Waker` backed by a per-task `TaskHandle`.

## Modules

| Module         | Purpose                                              |
|----------------|------------------------------------------------------|
| `executor`     | `Executor` — run-queue of `Task` futures.            |
| `waker`        | `CapletWaker` impl of `core::task::Wake`.            |
| `wake_source`  | Token a syscall uses to wake a specific task.        |
| `wake_queue`   | Lock-free MPSC queue of pending wake tokens.         |
| `block_on`     | Synchronous `block_on` for one-shot futures.         |
| `async_task`   | `AsyncTask` — the spawned future.                    |
| `spawner`      | `Spawner` used by macros.                            |

## Public API

```rust
use caplet_async::{Spawner, block_on};

let s = Spawner::new(executor.clone());
let h = s.spawn(async {
    // ...
});
h.await;
```

## Status

Stable. The `Waker` integrates with `core::task::Waker`
and is exercised by the M1 unit + Kani harness set.