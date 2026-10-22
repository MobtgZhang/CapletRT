//! Resource Table
//!
//! A `Cap<T>` references a *slot* in the CSpace, but the slot does
//! not own the resource itself — it just stores permissions and a
//! derivation link. The actual resource is stored in a `ResourceTable`
//! keyed by a resource ID.
//!
//! `Cap::resource_id()` is set when the slot is allocated (e.g. when
//! the user derives an endpoint cap, the kernel assigns a fresh
//! resource ID and registers the underlying primitive in this table).
//!
//! This crate provides the table and registration helpers; the actual
//! resource types are owned by the higher-level crates (channel,
//! socket, file, ...).

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};

/// Opaque resource ID. Uniquely identifies a registered resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(pub u32);

/// Trait for resources that can be stored in the table. The kernel
/// itself never inspects a resource; it just routes cleanup when the
/// resource is dropped.
pub trait Resource: Send + Sync {
    /// Optional cleanup hook invoked when the table removes the entry.
    /// The default is a no-op.
    fn on_drop(&self) {}
}

/// The kernel's resource table. Single instance lives in the kernel.
pub struct ResourceTable {
    inner: ::caplet_sync::SpinLock<BTreeMap<ResourceId, Arc<dyn Resource>>>,
    next_id: AtomicU32,
}

impl ResourceTable {
    /// Create a new empty resource table.
    pub const fn new() -> Self {
        Self {
            inner: ::caplet_sync::SpinLock::new(BTreeMap::new()),
            next_id: AtomicU32::new(1),
        }
    }

    /// Register a new resource and return its ID.
    pub fn register<R: Resource + 'static>(&self, r: R) -> ResourceId {
        let id = ResourceId(self.next_id.fetch_add(1, Ordering::Relaxed));
        self.inner.lock().insert(id, Arc::new(r));
        id
    }

    /// Look up a resource by ID.
    pub fn get(&self, id: ResourceId) -> Option<Arc<dyn Resource>> {
        self.inner.lock().get(&id).cloned()
    }

    /// Remove a resource. The Arc count is decremented; if this was
    /// the last reference, the resource's `on_drop` runs.
    pub fn unregister(&self, id: ResourceId) -> bool {
        if let Some(r) = self.inner.lock().remove(&id) {
            // Decrement count. If this is the last strong ref, run cleanup.
            if Arc::strong_count(&r) == 1 {
                r.on_drop();
            }
            true
        } else {
            false
        }
    }

    /// Number of registered resources.
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

impl Default for ResourceTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicU32;

    struct CountingResource {
        drops: Arc<AtomicU32>,
    }
    impl Resource for CountingResource {
        fn on_drop(&self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn register_and_unregister() {
        let drops = Arc::new(AtomicU32::new(0));
        let r = CountingResource {
            drops: Arc::clone(&drops),
        };
        let t = ResourceTable::new();
        let id = t.register(r);
        assert!(t.get(id).is_some());
        assert!(t.unregister(id));
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert!(t.is_empty());
    }

    #[test]
    fn multiple_resources_have_unique_ids() {
        let t = ResourceTable::new();
        let id1 = t.register(CountingResource {
            drops: Arc::new(AtomicU32::new(0)),
        });
        let id2 = t.register(CountingResource {
            drops: Arc::new(AtomicU32::new(0)),
        });
        assert_ne!(id1, id2);
        assert_eq!(t.len(), 2);
    }
}
