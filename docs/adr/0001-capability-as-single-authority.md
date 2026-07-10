# ADR-001: Capability as the Single Authorisation Primitive

Status: Accepted
Date: 2026-07-09
Supersedes: —

## Context

CapletRT must enforce least-privilege for every resource a task
touches: peripherals, IPC endpoints, filesystem handles, sockets,
and even other tasks' capabilities. A traditional POSIX-style
permission check (UID/GID + ACL) is both heavyweight on Cortex-M
and semantically weak: it ties authorisation to identity rather
than to the operation about to be performed.

## Decision

The only authorisation primitive in CapletRT is a typed
`Cap<T>` value, parameterised by the object type `T` and its
`Permissions` set. All kernel-facing syscalls (`syscall_cap_*`)
operate exclusively on `Cap<T>` handles and refuse anything
else. `Handle<T>` is the untyped in-process helper used by the
cap-table; `Cap<T>` is the user-facing wrapper that carries
rights.

## Consequences

- Every syscall path is uniform: validate `Cap<T>` → check
  `Permissions` → dispatch.
- Revocation is centralised in `CSpace`; deleting a `Cap<T>`
  invalidates every derived child cap (the parent link in
  `parent_link.rs`).
- `unsafe` is required only inside `Cap<T>::check()`; the rest
  of the kernel is safe Rust.
- The model implies an "ambient authority" rule: tasks never
  carry authority in their stack frame alone. A leaked cap
  loses authority the moment the cap-table slot is reclaimed.

## Alternatives considered

- **POSIX uid + ACL**: rejected — too much kernel state, weak
  semantics, and no static enforcement of rights.
- **seL4-style CNode**: rejected — semantically equivalent but
  considerably more verbose; we want a friendlier Rust API.
- **Capability-per-thread**: rejected — capabilities flow
  along IPC channels in CapletRT, not thread boundaries.