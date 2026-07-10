# `caplet-kernel`

The actual kernel: scheduler, syscall dispatcher, tickless
idle, boot path, and (in Phase 4+) the resource table.

## Modules

| Module             | Purpose                                                |
|--------------------|--------------------------------------------------------|
| `kernel`           | The `Kernel` struct — top-level container.             |
| `kernel_init`      | `cap_init` / `cap_init_with_config`.                    |
| `kernel_config`    | Static configuration (priority levels, time slice).    |
| `main_task`        | Boot and idle task constructors.                       |
| `scheduler`        | Priority-based scheduler.                               |
| `ready_queue`      | Per-priority ready queue.                               |
| `priority`         | Priority levels and priority-ceiling helpers.          |
| `syscall`          | Top-level syscall dispatch.                             |
| `syscall_cap`      | Capability syscalls (mint / derive / revoke).          |
| `syscall_task`     | Task syscalls (spawn / yield / exit).                  |
| `syscall_ipc`      | IPC syscalls (send / recv / notify).                   |
| `syscall_dispatch` | Routing table from syscall id to handler.              |
| `tickless`         | Tickless-idle counters and helpers.                    |
| `boot`             | Boot banner and reset hook.                            |
| `memory`           | Memory layout / kernel allocator policy.               |
| `bump_allocator`   | Tiny bump allocator used by the demo boards.           |
| `slot_allocator`   | Index-based slot allocator (used by the cap table).    |

## Build-time configuration

`KernelConfig::qemu()` provides sane defaults for the
`mps2-an386` machine. Boards may call `cap_init_with_config`
to override.

## Status

Active development; the M1 milestone is complete and the M2
work is layered on top.