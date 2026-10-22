//! Workspace target for the Kani proof harnesses.
//!
//! The actual proofs live in `tools/kani/harness/` so they remain
//! readable as standalone specifications.  They are compiled as
//! modules only under Kani, which keeps ordinary `cargo check` and
//! `cargo test` fast.

#![cfg_attr(not(kani), allow(missing_docs))]

// `include!` expands in item position, which makes the `//!` module
// documentation and inner `#![cfg(kani)]` attributes in each harness
// invalid.  `#[path]` modules give every harness a real module context.
#[cfg(kani)]
#[path = "../harness/cap_derive.rs"]
mod cap_derive;

#[cfg(kani)]
#[path = "../harness/derivation_priority_audit.rs"]
mod derivation_priority_audit;

#[cfg(kani)]
#[path = "../harness/loader_manifest.rs"]
mod loader_manifest;

#[cfg(kani)]
#[path = "../harness/mmu_permissions.rs"]
mod mmu_permissions;

#[cfg(kani)]
#[path = "../harness/mqtt_sn_state.rs"]
mod mqtt_sn_state;

#[cfg(kani)]
#[path = "../harness/net_dhcp_state.rs"]
mod net_dhcp_state;

#[cfg(kani)]
#[path = "../harness/net_socket_table.rs"]
mod net_socket_table;

#[cfg(kani)]
#[path = "../harness/smp_ipc.rs"]
mod smp_ipc;

#[cfg(kani)]
#[path = "../harness/syscall_dispatch.rs"]
mod syscall_dispatch;
