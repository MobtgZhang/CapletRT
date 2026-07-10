# `caplet-cap`

The capability primitives for CapletRT.

`CapletRT` borrows the `Cap<T>`-style vocabulary from
seL4 / Capsicum / Wuffs: every reference to a kernel-managed
resource is a typed, unforgeable, attenuable capability.

## Modules

| Module           | Purpose                                                    |
|------------------|------------------------------------------------------------|
| `cap`            | The `Cap<T>` wrapper.                                      |
| `handle`         | Opaque resource identifier (`HandleIndex` + `Generation`).  |
| `permissions`    | Read/write/grow/admin permission bits.                     |
| `safety_bits`    | Safety bits — sentinels that tasks toggle before passing.  |
| `attenuation`    | Derivation rules (drop permissions, mint children).        |
| `derivation`     | `cap_derive` / `derive_with_perms`.                        |
| `revocation`     | Slow-path and fast-path revocation.                        |
| `cspace`         | Per-task CSpace — table of owned capabilities.             |
| `parent_link`    | Parent-child relationships in the capability tree.         |
| `epoch`          | Epoch-based bulk revocation.                               |
| `rights`         | Per-resource-class rights vocabulary.                      |
| `error`          | `CapError` / `CapResult<T>`.                                |

## Public API (subset)

```rust
use caplet_cap::{Cap, cspace::CSpace, permissions::Permissions};

let mut root_cspace = CSpace::new();
let cap: Cap<Uart> = root_cspace.mint(/* permissions */);
let child: Cap<Uart> = root_cspace.derive(&cap, Permissions::READ)?;
```

## Verification

The crate is the primary target for Kani proofs
(`tools/kani/harness/`). All public methods that mutate a
`Cap` (derivation, attenuation, revocation) have an associated
harness asserting the relevant safety property.

## Status

Stable — covered by the Phase 1 Kani harness suite.