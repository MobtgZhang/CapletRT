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

- **No ambient dynamic loading.** Static linking remains the default, but
  `caplet-loader` also supports explicit `.capp`/ELF module loading. A module
  is admitted only after header validation, manifest capability checks, and
  (when `FLAG_SIGNED` is set) Ed25519 signature verification.
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
under `tools/kani/harness/`. The current catalogue contains 39 proof
functions, including:

- `cap_derive_preserves_parent_metadata` and
  `attenuation_is_monotonic` — derivation preserves parent metadata and
  cannot gain permissions.
- `restrictive_safety_idempotent_on_parent`,
  `safety_attenuate_drops_unset_bits`, and
  `audit_bit_propagates_correctly` — safety and audit bits are restrictive.
- `priority_ceiling_of_equal`, `priority_ceiling_picks_higher`, and
  `priority_total_and_reflexive` — priority ordering and ceiling behaviour.
- `pinned_affinity_admits_only_its_owner` — affinity masks cannot be bypassed.
- `socket_table_open_respects_capacity`, the DHCP state proofs, and
  `mqtt_sn_state_legal` — network state/capacity boundaries.
- Fourteen syscall-dispatch proofs covering syscall-number round-trips,
  unknown syscalls, argument rejection, namespace/module/crypto/flash
  capability checks, and well-formed dispatch results. See `docs/kani.md` for
  the exact list.

See `docs/kani.md` for the harness catalogue.

## Threat model

| Threat                                   | Mitigation                       |
|------------------------------------------|----------------------------------|
| Malicious task accesses another task's memory | No ambient authority; no shared mutable state without a cap. |
| Task refuses to release a resource       | `Cap<T>::Drop` triggers cleanup; resource table (Phase 4) is the safety net. |
| Forged capability                        | Caps are unforgeable; the cap table is only writable from kernel space. |
| Replay / downgrade attack on permissions | Revocation cascade; attenuation is monotonic. |
| Kernel compromise via tainted input      | Syscall handlers validate arguments before mutating state; Kani checks the documented dispatch and argument-boundary properties. |

## Out of scope (M2)

- **Side-channel resistance.** The kernel is written in plain
  Rust with constant-time considerations deferred to a future
  milestone.
- **Cryptographic attestation.** No TPM or secure-element attestation chain is
  implemented. Software SHA-256, AES-128, and Ed25519 verification are real
  implementations in `caplet-crypto`; hardware-backed key storage and remote
  attestation remain future work.
