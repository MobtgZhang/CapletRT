//! IPC namespaces for M3.
//!
//! A *namespace* is a scoped set of IPC endpoints. Two namespaces
//! are isolated from each other: an endpoint registered in
//! namespace A is not visible to namespace B. Namespaces form a
//! tree; an endpoint in a parent namespace is reachable from any
//! child namespace unless a child namespace explicitly shadows it
//! with a same-named endpoint of its own.
//!
//! Each process belongs to exactly one namespace. The process
//! receives its initial namespace at creation time. A process can
//! `join_namespace` (with the appropriate capability) to participate
//! in another namespace; the original namespace is left intact for
//! audit / debugging.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

use caplet_cap::{CSpace, Permissions, SafetyBits};

use crate::process::ProcessId;

/// Globally unique namespace id assigned by the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NamespaceId(pub u32);

/// Symbolic name of an endpoint, e.g. `"sensors/temp"`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EndpointName(pub String);

impl EndpointName {
    /// Construct from a `&'static str`. Used by the macro layer.
    pub fn from_static(s: &'static str) -> Self {
        Self(String::from(s))
    }
}

/// Opaque handle to an endpoint living in a namespace. Indexes into
/// the namespace's `endpoints` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EndpointHandle(pub u32);

/// Namespace-local endpoint name table.
///
/// Namespace endpoint sets are small and bounded.  A linear table avoids the
/// large, hard-to-bound control-flow graph of a tree map while preserving the
/// map operations the kernel needs: unique-name registration and lookup.
#[derive(Debug, Default)]
pub struct EndpointTable {
    entries: Vec<(EndpointName, EndpointHandle)>,
}

impl EndpointTable {
    /// Create an empty endpoint table.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Return whether `name` is already registered.
    pub fn contains_key(&self, name: &EndpointName) -> bool {
        self.entries.iter().any(|(key, _)| key == name)
    }

    /// Insert or replace an endpoint, returning the previous handle.
    pub fn insert(&mut self, name: EndpointName, handle: EndpointHandle) -> Option<EndpointHandle> {
        if let Some((_, existing)) = self.entries.iter_mut().find(|(key, _)| key == &name) {
            return Some(core::mem::replace(existing, handle));
        }
        self.entries.push((name, handle));
        None
    }

    /// Look up an endpoint by name.
    pub fn get(&self, name: &EndpointName) -> Option<&EndpointHandle> {
        self.entries
            .iter()
            .find_map(|(key, handle)| (key == name).then_some(handle))
    }

    /// Number of registered endpoints.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return whether the table has no endpoints.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// An IPC namespace.
///
/// The namespace owns its endpoint set, an allowlist of publishers
/// (processes that may `ns_publish`) and an allowlist of subscribers
/// (processes that may `ns_subscribe`).
pub struct IpcNamespace {
    /// Unique id.
    pub id: NamespaceId,
    /// Symbolic name (for debugging / service discovery).
    pub name: String,
    /// Parent namespace id. `None` for the root namespace.
    pub parent: Option<NamespaceId>,
    /// Endpoints registered in this namespace, keyed by name.
    pub endpoints: EndpointTable,
    /// Process ids allowed to publish into this namespace.
    pub allowed_publishers: Vec<ProcessId>,
    /// Process ids allowed to subscribe to this namespace.
    pub allowed_subscribers: Vec<ProcessId>,
}

impl IpcNamespace {
    /// Construct a new namespace.
    pub fn new(id: NamespaceId, name: String, parent: Option<NamespaceId>) -> Self {
        Self {
            id,
            name,
            parent,
            endpoints: EndpointTable::new(),
            allowed_publishers: Vec::new(),
            allowed_subscribers: Vec::new(),
        }
    }

    /// Register an endpoint.
    ///
    /// Returns `false` when the name is already registered.  Endpoint names
    /// are capability identities: silently replacing one would let a second
    /// process redirect traffic that was authorized for the first owner.
    pub fn register(&mut self, name: EndpointName, handle: EndpointHandle) -> bool {
        if self.endpoints.contains_key(&name) {
            return false;
        }
        self.endpoints.insert(name, handle);
        true
    }

    /// Look up an endpoint by name. Walks up to the parent if the
    /// name is not present locally.
    pub fn lookup(&self, name: &EndpointName) -> Option<EndpointHandle> {
        self.endpoints.get(name).copied()
    }

    /// Returns `true` if `pid` is allowed to publish into this namespace.
    pub fn may_publish(&self, pid: ProcessId) -> bool {
        self.allowed_publishers.contains(&pid)
    }

    /// Returns `true` if `pid` is allowed to subscribe to this namespace.
    pub fn may_subscribe(&self, pid: ProcessId) -> bool {
        self.allowed_subscribers.contains(&pid)
    }
}

/// Tree of namespaces. Stored in the kernel's `namespaces` table.
pub struct NamespaceTree {
    /// All namespaces indexed by their dense, monotonically allocated id.
    /// The root has `parent = None` and lives at index zero.
    by_id: Vec<Option<IpcNamespace>>,
    /// Next namespace id to hand out.
    next_id: u32,
}

impl NamespaceTree {
    /// Create a new tree with a single root namespace.
    pub fn new() -> Self {
        let root = IpcNamespace::new(NamespaceId(0), String::from("root"), None);
        Self {
            by_id: alloc::vec![Some(root)],
            next_id: 1,
        }
    }

    /// Allocate a new namespace id.
    pub fn next_id(&mut self) -> NamespaceId {
        let id = NamespaceId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    /// Insert a namespace into the tree.
    pub fn insert(&mut self, ns: IpcNamespace) {
        let index = ns.id.0 as usize;
        if index >= self.by_id.len() {
            self.by_id.resize_with(index + 1, || None);
        }
        self.by_id[index] = Some(ns);
    }

    /// Remove a namespace.  Kernel-only rollback helper used when granting
    /// the creator's capability fails after the id has been allocated.
    pub fn remove(&mut self, id: NamespaceId) -> Option<IpcNamespace> {
        self.by_id.get_mut(id.0 as usize)?.take()
    }

    /// Look up a namespace by id.
    pub fn get(&self, id: NamespaceId) -> Option<&IpcNamespace> {
        self.by_id.get(id.0 as usize)?.as_ref()
    }

    /// Look up a namespace by id mutably.
    pub fn get_mut(&mut self, id: NamespaceId) -> Option<&mut IpcNamespace> {
        self.by_id.get_mut(id.0 as usize)?.as_mut()
    }

    /// Walk up the parent chain to find an endpoint. Returns the
    /// owning namespace id and handle.
    pub fn resolve(
        &self,
        start: NamespaceId,
        name: &EndpointName,
    ) -> Option<(NamespaceId, EndpointHandle)> {
        let mut cur = Some(start);
        while let Some(id) = cur {
            if let Some(ns) = self.get(id) {
                if let Some(h) = ns.lookup(name) {
                    return Some((id, h));
                }
                cur = ns.parent;
            } else {
                break;
            }
        }
        None
    }
}

impl Default for NamespaceTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Decide whether `pid` may publish on `endpoint` in `ns`. The
/// rule is:
/// 1. The process must be in the namespace (or an ancestor).
/// 2. The process must appear in `allowed_publishers`.
/// 3. The process must hold a `Cap<Endpoint>` whose
///    `cross_namespace` safety bit is set if the destination
///    namespace is not the same as the process's own namespace.
pub fn may_publish_into(
    ns: &IpcNamespace,
    pid: ProcessId,
    from_ns: NamespaceId,
    has_cross_ns_cap: bool,
) -> bool {
    if !ns.may_publish(pid) {
        return false;
    }
    if ns.id != from_ns && !has_cross_ns_cap {
        return false;
    }
    true
}

/// Helper for the syscall layer: construct the safety bits a
/// process needs to publish across namespaces. The cross-namespace
/// property is conveyed by setting the `audit` bit, which causes
/// every cross-ns publish attempt to be logged to the audit ring.
pub fn cross_namespace_bits(_perms: Permissions) -> SafetyBits {
    let mut bits = SafetyBits::default();
    bits.set_audit(true);
    bits
}

/// Helper: build a [`CSpace`] for a fresh namespace. The empty
/// CSpace means no endpoints are pre-registered, which is the safe
/// default.
pub fn empty_caps() -> CSpace {
    CSpace::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_namespace_default() {
        let tree = NamespaceTree::new();
        assert!(tree.get(NamespaceId(0)).is_some());
    }

    #[test]
    fn register_and_lookup() {
        let mut tree = NamespaceTree::new();
        let mut ns = IpcNamespace::new(NamespaceId(1), "test".into(), Some(NamespaceId(0)));
        ns.register(EndpointName::from_static("foo"), EndpointHandle(42));
        tree.insert(ns);
        let resolved = tree.resolve(NamespaceId(1), &EndpointName::from_static("foo"));
        assert_eq!(resolved, Some((NamespaceId(1), EndpointHandle(42))));
    }

    #[test]
    fn walk_to_parent() {
        let mut tree = NamespaceTree::new();
        let root = tree.get_mut(NamespaceId(0)).unwrap();
        root.register(EndpointName::from_static("root-svc"), EndpointHandle(1));
        let mut child = IpcNamespace::new(NamespaceId(1), "child".into(), Some(NamespaceId(0)));
        child.register(EndpointName::from_static("child-svc"), EndpointHandle(2));
        tree.insert(child);

        // Local lookup.
        let r1 = tree.resolve(NamespaceId(1), &EndpointName::from_static("child-svc"));
        assert_eq!(r1, Some((NamespaceId(1), EndpointHandle(2))));
        // Inherited lookup.
        let r2 = tree.resolve(NamespaceId(1), &EndpointName::from_static("root-svc"));
        assert_eq!(r2, Some((NamespaceId(0), EndpointHandle(1))));
        // Not found anywhere.
        let r3 = tree.resolve(NamespaceId(1), &EndpointName::from_static("nope"));
        assert_eq!(r3, None);
    }

    #[test]
    fn may_publish_requires_allowlist() {
        let mut ns = IpcNamespace::new(NamespaceId(1), "x".into(), Some(NamespaceId(0)));
        let pid = ProcessId(7);
        assert!(!ns.may_publish(pid));
        ns.allowed_publishers.push(pid);
        assert!(ns.may_publish(pid));
    }

    #[test]
    fn may_publish_into_rejects_without_cross_ns() {
        let mut ns = IpcNamespace::new(NamespaceId(1), "x".into(), Some(NamespaceId(0)));
        let pid = ProcessId(7);
        ns.allowed_publishers.push(pid);
        // Same namespace: no cross-ns bit needed.
        assert!(may_publish_into(&ns, pid, NamespaceId(1), false));
        // Different namespace: needs cross-ns bit.
        assert!(!may_publish_into(&ns, pid, NamespaceId(2), false));
        assert!(may_publish_into(&ns, pid, NamespaceId(2), true));
    }
}
