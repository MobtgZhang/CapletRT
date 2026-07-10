# CapletRT Security Model

This document describes the security guarantees of CapletRT and
the threat model they address.

## Trust boundaries

```
+------------------+          +-----------------+
|  Task code       |          |  Kernel code    |
|  (user space)    |          |  (privileged)   |
|                  |          |                 |
|  Cap<T> values   |  syscall |  CapTable,      |
|  are the only    |  --->    |  CSpace,        |
|  authority.      |  <---    |  Scheduler.     |
+------------------+  return  +-----------------+
```

The kernel is the only code allowed to be `unsafe`. Everything
above is plain Rust.

## Capability invariants

Every `Cap<T>` value satisfies:

1. **No ambient authority.** A task that does not hold a
   `Cap<T>` for `T` cannot perform any operation on `T`. There
   is no fallback "root" path.
2. **Attenuation.** A `Cap<T>` may only have *fewer*
   permissions than its parent. Re-attempting to grant a
   permission not present in the parent cap is rejected by
   `derivation.rs`.
3. **Revocation is total.** When a parent cap is revoked, all
   derived caps become invalid and any future syscall using
   them returns `Err(CapRevoked)`.
4. **No aliasing of mutable state.** A `Cap<&mut T>` may be
   held by at most one task at a time; the kernel tracks
   exclusivity in the resource table (Phase 4).

These properties are enforced by `caplet-cap` and form the
contract that every other crate relies on.

## What the kernel will not do

- **No dynamic loading.** All code is statically linked; the
  kernel does not execute untrusted binaries.
- **No mprotect-style tricks.** MPU configuration is static
  and per-task; the kernel never re-programs MPU regions at
  runtime to grant or revoke memory.
- **No silent failures.** Every denied syscall emits an audit
  record (`audit.rs`).

## Audit log

`caplet-cap::audit` maintains a ring buffer of the last `N`
capability operations. The buffer is exported via a `CapletHandle`
so that a privileged task can forward it to the field bus.

## Kani verification

The capability invariants above are encoded as Kani harnesses
under `tools/kani/harness/`:

- `cap_derivation_attenuation.rs` — derived caps cannot gain
  permissions.
- `cap_revocation_cascade.rs` — revoking a parent invalidates
  every child.
- `priority_ceiling.rs` — protocol correctness.
- `socket_table_capacity.rs` — socket table respects the cap.
- Plus 25 syscall harnesses for `cap_task_*`, `cap_ipc_*`,
  `cap_fs_*`, `cap_net_*`.

See `docs/kani.md` for the harness catalogue.

## Threat model

| Threat                                   | Mitigation                       |
|------------------------------------------|----------------------------------|
| Malicious task accesses another task's memory | No ambient authority; no shared mutable state without a cap. |
| Task refuses to release a resource       | `Cap<T>::Drop` triggers cleanup; resource table (Phase 4) is the safety net. |
| Forged capability                        | Caps are unforgeable; the cap table is only writable from kernel space. |
| Replay / downgrade attack on permissions | Revocation cascade; attenuation is monotonic. |
| Kernel compromise via tainted input      | All syscalls validate arguments before mutating state. Kani proves this for the 25 paths. |

## Out of scope (M2)

- **Side-channel resistance.** The kernel is written in plain
  Rust with constant-time considerations deferred to a future
  milestone.
- **Cryptographic attestation.** The M2 demo does not include
  a TPM or secure element integration; `caplet-hal::crypto` is
  stubbed.