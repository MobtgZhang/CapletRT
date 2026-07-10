# Kani Verification Catalogue

This document describes the formal-verification harness set
that ships with CapletRT. Every harness is a small Rust file
under `tools/kani/harness/` that the `cargo kani` driver runs
in CI.

## How to run

```bash
cargo install --locked kani-verifier
cargo kani --harness <name>
```

The CI workflow `kani` job installs the verifier and runs the
full catalogue in parallel.

## Catalogue

### Capability invariants

| Harness                                | Property                                                |
|----------------------------------------|---------------------------------------------------------|
| `cap_derivation_attenuation.rs`        | A derived cap cannot gain permissions over its parent.  |
| `cap_revocation_cascade.rs`            | Revoking a parent cap invalidates every child.          |
| `cap_exclusive_borrow.rs`              | `Cap<&mut T>` is held by at most one task.              |

### Scheduler + IPC

| Harness                                | Property                                                |
|----------------------------------------|---------------------------------------------------------|
| `priority_ceiling.rs`                  | No priority inversion across the ceiling protocol.      |
| `ipc_no_lost_wakeup.rs`                | Every notify() reaches a runnable task.                 |
| `tickless_64bit_counter.rs`            | The 32+32 atomic counter reconstructs the 64-bit tick.  |

### Filesystem

| Harness                                | Property                                                |
|----------------------------------------|---------------------------------------------------------|
| `fs_handle_lifecycle.rs`               | `Cap<MountPoint>` revoke closes every derived handle.   |
| `ramfs_consistency.rs`                 | Concurrent reads see a consistent file tree.            |

### Network

| Harness                                | Property                                                |
|----------------------------------------|---------------------------------------------------------|
| `net_socket_table.rs`                  | `SocketTable::open` respects `MAX_SOCKETS`.             |
| `net_dhcp_state.rs`                    | DHCP state machine never leaves the legal set.          |
| `mqtt_sn_state.rs`                     | MQTT-SN client never publishes before REGISTER ack.     |

### 25 syscall paths

Every `cap_*` syscall has a dedicated harness:

```
cap_alloc         cap_derive        cap_revoke        cap_grant
cap_send          cap_recv          cap_notify        cap_call

task_spawn        task_exit         task_yield        task_kill
task_set_priority task_set_budget   task_block        task_unblock

fs_open           fs_close          fs_read           fs_write
fs_seek
net_open          net_bind          net_recv          net_send
net_close
```

That is 24 + 1 (the audit syscall) = 25 harnesses.

## Why Kani, not MIRAI

- **Soundness**: Kani produces a CBMC-based proof; MIRAI is a
  dataflow analyser that can miss bugs.
- **Portability**: Kani ships pre-built for Linux; MIRAI
  requires nightly toolchain alignment.
- **Result format**: Kani's counter-example traces integrate
  with `cargo test` reports.

## Adding a new harness

1. Create `tools/kani/harness/<area>_<name>.rs`.
2. Annotate the entry function with `#[kani::proof]`.
3. Add the harness name to `tools/kani/runner.rs`.
4. Push and confirm the `kani` job is green.