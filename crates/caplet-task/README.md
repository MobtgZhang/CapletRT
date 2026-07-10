# `caplet-task`

The user-facing task layer on top of `caplet-async` and
`caplet-cap`.

## Modules

| Module         | Purpose                                         |
|----------------|-------------------------------------------------|
| `task`         | `Task` — the kernel-visible task handle.        |
| `task_id`      | `TaskId` newtype.                               |
| `task_state`   | `TaskState` (Ready / Running / Blocked / Dead). |
| `task_config`  | Static task configuration (priority, stack).    |
| `task_builder` | Fluent builder for spawning tasks.              |
| `task_context` | The per-task context block stored in a slot.    |
| `task_ref`     | Borrow-checker-safe cross-task references.      |
| `caplets`      | `Caplet` — task-typed capability handle.        |

## Lifecycle

A `Task` is created via `task_builder`, registered in the
kernel's task table, and dropped when the kernel reaps it.
The cross-task reference machinery in `task_ref` is what
enables `Cap<Endpoint>` to live across tasks without `unsafe`.

## Status

Stable.