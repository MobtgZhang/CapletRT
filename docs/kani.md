# Kani Verification Catalogue

CapletRT's formal-verification catalogue lives in
`tools/kani/harness/`. The proofs are compiled through the
`caplet-kani-harnesses` crate and are intentionally small enough to run one
at a time, so a single failing property cannot hide the rest of the
catalogue.

## How to run

```bash
cargo install --locked kani-verifier
cargo-kani setup

# One proof by Rust function name:
cargo kani -p caplet-kani-harnesses --harness attenuation_is_monotonic

# Every proof, aggregated by the checked-in script:
bash tools/kani/run_catalogue.sh
```

`run_catalogue.sh` extracts every `#[kani::proof]` function, including proofs
that carry additional attributes such as `#[kani::unwind(16)]`, runs each one
separately, and fails if any proof fails. A missing `cargo-kani` executable is
reported as `UNVERIFIED` and exits non-zero; it is never treated as a pass.

## Catalogue

There are currently 39 proof functions across nine harness modules.

### Capability derivation

| Module | Proofs |
|--------|--------|
| `cap_derive.rs` | `cap_derive_preserves_parent_metadata` |
| `derivation_priority_audit.rs` | `attenuation_is_monotonic`, `is_valid_attenuation_iff_subset`, `safety_attenuate_drops_unset_bits`, `restrictive_safety_idempotent_on_parent`, `priority_total_and_reflexive`, `priority_ceiling_of_equal`, `priority_ceiling_picks_higher`, `audit_bit_propagates_correctly` |

These cover attenuation monotonicity, safety-bit restrictions, priority
ordering/ceiling behaviour, and audit-bit propagation.

### Loader header

| Module | Proofs |
|--------|--------|
| `loader_manifest.rs` | `parse_capp_rejects_non_magic_byte0`, `parse_capp_rejects_non_magic_byte1`, `parse_capp_rejects_non_magic_byte2`, `parse_capp_rejects_non_magic_byte3`, `parse_capp_proceeds_past_magic` |

The four rejection proofs partition every non-magic prefix by the position of
its first mismatching byte. They exercise `validate_capp_header`, which is the
fixed-size gate called by `parse_capp` before any allocation, string decoding,
relocation processing, or capability-section parsing.

### MMU permissions

| Module | Proofs |
|--------|--------|
| `mmu_permissions.rs` | `kernel_page_never_grants_user_access`, `read_only_user_page_maps_to_ap_user_ro`, `writable_user_page_maps_to_ap_user_rw`, `device_page_sets_strongly_ordered_attributes` |

### Scheduler, IPC and SMP

| Module | Proofs |
|--------|--------|
| `smp_ipc.rs` | `pinned_affinity_admits_only_its_owner`, `cross_namespace_publish_requires_allowlist_and_capability` |

### Syscall dispatch

| Module | Proofs |
|--------|--------|
| `syscall_dispatch.rs` | `every_syscall_number_round_trips`, `unknown_syscall_always_errors`, `log_write_is_side_effect_free`, `task_spawn_rejects_zero_arguments`, `task_spawn_rejects_stack_overflow`, `capability_lookup_rejects_null_name`, `duplicate_and_derive_without_live_parent_error`, `memory_operations_reject_zero_size_or_address`, `timer_operations_reject_zero_arguments`, `ipc_operations_reject_null_buffers`, `namespace_join_rejects_missing_namespace`, `module_syscalls_reject_missing_capability`, `crypto_and_flash_reject_missing_grants`, `dispatch_returns_well_formed_result` |

The syscall enum has 38 variants: 37 operations plus the `None` sentinel.
The proof catalogue does not claim one harness per syscall; it covers the
numeric round-trip for the full enum and selected security/argument boundaries
for the handlers listed above.

### Network protocols

| Module | Proofs |
|--------|--------|
| `mqtt_sn_state.rs` | `mqtt_sn_state_legal` |
| `net_dhcp_state.rs` | `dhcp_shrunk_idle_legal`, `dhcp_shrunk_discover_legal`, `dhcp_shrunk_bound_legal` |
| `net_socket_table.rs` | `socket_table_open_respects_capacity` |

## Adding a proof

1. Add a `#[kani::proof]` function to the appropriate module in
   `tools/kani/harness/`, or register a new module in `tools/kani/src/lib.rs`.
2. Keep the property self-contained and bound any loops with
   `#[kani::unwind(N)]` when the default bound is insufficient.
3. Run `bash tools/kani/run_catalogue.sh`; the script discovers the new
   function automatically.
4. Update this catalogue and the proof count.

## Why Kani

- Kani produces a CBMC-backed proof or a concrete counterexample.
- It runs on stable-toolchain-adjacent Kani releases in Linux CI.
- A failed or missing verifier is a hard CI failure, not a skipped test.
