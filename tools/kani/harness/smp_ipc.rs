//! SMP affinity and cross-namespace IPC authorization.
//!
//! Property to prove: a pinned affinity mask admits no CPU other than its
//! owner, and a cross-namespace publish is allowed only when the caller was
//! explicitly allow-listed and presents the cross-namespace capability.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use caplet_kernel::cpu::CpuId;
use caplet_kernel::ipc_namespace::{may_publish_into, EndpointTable, IpcNamespace, NamespaceId};
use caplet_kernel::process::ProcessId;
use caplet_kernel::smp_scheduler::AffinityMask;

#[kani::proof]
fn pinned_affinity_admits_only_its_owner() {
    let owner = CpuId::new(kani::any());
    let candidate = CpuId::new(kani::any());
    if AffinityMask::pinned(owner).allows(candidate) {
        assert_eq!(candidate, owner);
    }
}

#[kani::proof]
fn cross_namespace_publish_requires_allowlist_and_capability() {
    let pid = ProcessId(kani::any());
    let allow_listed: bool = kani::any();
    let same_namespace: bool = kani::any();
    let has_cross_ns_cap: bool = kani::any();

    let mut allowed_publishers = Vec::new();
    if allow_listed {
        allowed_publishers.push(pid);
    }
    let ns = IpcNamespace {
        id: NamespaceId(1),
        name: String::new(),
        parent: Some(NamespaceId(0)),
        endpoints: EndpointTable::new(),
        allowed_publishers,
        allowed_subscribers: Vec::new(),
    };
    let from_ns = if same_namespace {
        NamespaceId(1)
    } else {
        NamespaceId(2)
    };

    let allowed = may_publish_into(&ns, pid, from_ns, has_cross_ns_cap);
    assert_eq!(
        allowed,
        allow_listed && (same_namespace || has_cross_ns_cap)
    );
}
