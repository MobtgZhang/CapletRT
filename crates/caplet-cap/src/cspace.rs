//! CSpace - Capability Space implementation
//!
//! A CSpace owns the *metadata* for capabilities (permissions, safety bits,
//! derivation links and generations).  Resources live in typed
//! [`Cap`](crate::Cap) objects and are linked to their slot through a
//! revocation guard.  This keeps the table architecture independent while
//! letting a revocation invalidate every outstanding reference immediately.

use crate::cap::{Cap, CapGuard, ResourceCell};
use crate::epoch::{Epoch, EpochTracker, SlotMetadata};
use crate::error::{CapError, CapResult};
use crate::handle::{Generation, Handle, HandleIndex};
use crate::parent_link::DerivationNode;
use crate::permissions::{perm, Permissions};
use crate::safety_bits::SafetyBits;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicU32, Ordering};

/// Maximum number of slots in a CSpace
pub const MAX_CSPACE_SLOTS: usize = 256;

/// Bitmask size for fast-path slot lookup
pub const SLOT_BITMAP_SIZE: usize = MAX_CSPACE_SLOTS / 64;

/// Mutable state protected by the CSpace lock.
pub(crate) struct CSpaceInner {
    // A slot is sparse: most entries in a typical CSpace are free.  Keeping
    // the full `Slot` inline made every empty CSpace initialise and tear down
    // hundreds of `Arc<dyn CapGuard>`/`String` fields, which is both wasteful
    // on the target and produces needlessly large verification state.
    slots: Vec<Option<Box<Slot>>>,
    free_bitmap: [u64; SLOT_BITMAP_SIZE],
    epoch_tracker: EpochTracker,
    active_count: usize,
    max_slots: usize,
    /// Generation to place in the next handle for a slot.  It is bumped on
    /// every free so stale handles cannot alias a newly allocated capability.
    generations: [u32; MAX_CSPACE_SLOTS],
}

/// CSpace - the capability space for a task.
///
/// The handle is cheap to clone, but all clones refer to the same capability
/// table.  This is useful for capabilities that need to release their slot
/// from `Drop` without manufacturing a `&'static mut CSpace`.
#[derive(Clone)]
pub struct CSpace {
    pub(crate) inner: Arc<SpinLock<CSpaceInner>>,
}

impl CSpace {
    /// Create a new empty CSpace
    pub fn new() -> Self {
        Self::with_max_slots(MAX_CSPACE_SLOTS)
    }

    /// Create a CSpace with a custom maximum slot count
    pub fn with_max_slots(max: usize) -> Self {
        let clamped_max = max.min(MAX_CSPACE_SLOTS);
        let mut free_bitmap = [u64::MAX; SLOT_BITMAP_SIZE];
        if clamped_max & 63 != 0 {
            let last_word = clamped_max / 64;
            let used_bits = clamped_max & 63;
            free_bitmap[last_word] &= (1u64 << used_bits) - 1;
        }
        Self {
            inner: Arc::new(SpinLock::new(CSpaceInner {
                slots: Vec::new(),
                free_bitmap,
                epoch_tracker: EpochTracker::new(),
                active_count: 0,
                max_slots: clamped_max,
                generations: [0; MAX_CSPACE_SLOTS],
            })),
        }
    }

    /// Allocate a new metadata-only slot.
    ///
    /// The returned handle deliberately carries no resource.  Use
    /// [`CSpace::insert_cap`] when the slot should grant access to a typed
    /// resource.
    pub fn allocate(
        &mut self,
        permissions: Permissions,
        safety_bits: SafetyBits,
        parent: Option<HandleIndex>,
    ) -> CapResult<Handle<u32>> {
        let mut inner = self.inner.lock();
        inner.allocate_slot(permissions, safety_bits, parent, None)
    }

    /// Allocate a slot backed by `resource` and return a capability bound to
    /// this CSpace.
    pub fn insert_cap<T: Send + 'static>(
        &mut self,
        resource: T,
        permissions: Permissions,
        safety_bits: SafetyBits,
        parent: Option<HandleIndex>,
    ) -> CapResult<Cap<T>> {
        let cell = ResourceCell::new(resource);
        let guard: Arc<dyn CapGuard> = cell.clone();
        let handle = {
            let mut inner = self.inner.lock();
            inner.allocate_slot(permissions, safety_bits, parent, Some(guard))?
        };
        let handle = Handle::<T>::new(handle.index(), handle.generation());
        Ok(Cap::bound(
            handle,
            permissions,
            safety_bits,
            cell,
            Arc::downgrade(&self.inner),
        ))
    }

    /// Derive a child capability from `parent`, enforcing that both
    /// permissions and safety bits are attenuated.
    pub fn derive_cap<T: Send + 'static>(
        &mut self,
        parent: &Cap<T>,
        permissions: Permissions,
        safety_bits: SafetyBits,
    ) -> CapResult<Cap<T>> {
        let parent_index = parent.handle().index();
        let resource = parent.resource().ok_or(CapError::Invalid)?;
        let guard: Arc<dyn CapGuard> = resource.clone();
        let handle = {
            let mut inner = self.inner.lock();
            inner.allocate_slot(permissions, safety_bits, Some(parent_index), Some(guard))?
        };
        let handle = Handle::<T>::new(handle.index(), handle.generation());
        resource.retain();
        Ok(Cap::bound(
            handle,
            permissions,
            safety_bits,
            resource,
            Arc::downgrade(&self.inner),
        ))
    }

    /// Look up a capability by handle and return a snapshot of its metadata.
    pub fn lookup<T>(&self, handle: Handle<T>) -> CapResult<SlotSnapshot> {
        let inner = self.inner.lock();
        inner
            .lookup_slot(handle.index(), handle.generation())
            .map(Into::into)
    }

    /// Read a slot's permissions and safety bits by raw index.
    pub fn slot_perms_safety(&self, index: u32) -> Option<(Permissions, SafetyBits)> {
        let inner = self.inner.lock();
        let slot = inner.slots.get(index as usize)?.as_ref()?;
        if !slot.metadata.is_valid(&inner.epoch_tracker.current()) {
            return None;
        }
        Some((slot.permissions, slot.safety_bits))
    }

    /// Read a slot's generation by raw index.
    pub fn slot_generation(&self, index: HandleIndex) -> Option<Generation> {
        let inner = self.inner.lock();
        let slot = inner.slots.get(index as usize)?.as_ref()?;
        if !slot.metadata.is_valid(&inner.epoch_tracker.current()) {
            return None;
        }
        Some(slot.generation)
    }

    /// Copy the live slot at `source_index` into `target`.
    ///
    /// This is the cross-CSpace primitive behind `cap_grant`.  It copies
    /// the authorization metadata and clones the revocation guard so the
    /// destination capability controls the same underlying resource.  A
    /// new generation and derivation root are allocated in the target;
    /// the source slot is left untouched (grant is a copy, while revoke
    /// remains the operation that removes a slot).
    ///
    /// The source and target must be distinct CSpaces: locking the same
    /// spinlock twice would deadlock, and a same-CSpace grant is better
    /// expressed with [`Self::allocate`] or [`Self::derive_cap`].
    pub fn clone_slot_into(
        &self,
        source_index: HandleIndex,
        target: &mut CSpace,
    ) -> CapResult<Handle<u32>> {
        if Arc::ptr_eq(&self.inner, &target.inner) {
            return Err(CapError::Invalid);
        }

        // Snapshot while holding only the source lock, then drop it before
        // taking the target lock.  This keeps the lock order acyclic.
        let (permissions, safety_bits, guard, name) = {
            let inner = self.inner.lock();
            let current_epoch = inner.epoch_tracker.current();
            let slot = inner
                .slots
                .get(source_index as usize)
                .and_then(Option::as_ref)
                .ok_or(CapError::NotFound)?;
            if !slot.metadata.is_valid(&current_epoch) {
                return Err(CapError::Revoked);
            }
            (
                slot.permissions,
                slot.safety_bits,
                slot.guard.clone(),
                slot.name.clone(),
            )
        };

        let handle = target
            .inner
            .lock()
            .allocate_slot(permissions, safety_bits, None, guard)?;
        if let Some(name) = name {
            if let Err(error) = target.bind_name(handle.index(), &name) {
                // The slot was allocated by this operation.  Roll it back
                // without invalidating the shared resource: the source slot
                // may still be live if binding failed for a transient reason.
                target.inner.lock().remove_slot(handle.index(), false).ok();
                return Err(error);
            }
        }
        Ok(handle)
    }

    /// Assign a discoverable name to a slot.
    pub fn bind_name(&self, index: HandleIndex, name: &str) -> CapResult<()> {
        let mut inner = self.inner.lock();
        let current_epoch = inner.epoch_tracker.current();
        let slot = inner
            .slots
            .get_mut(index as usize)
            .and_then(Option::as_mut)
            .ok_or(CapError::NotFound)?;
        if !slot.metadata.is_valid(&current_epoch) {
            return Err(CapError::Revoked);
        }
        slot.name = Some(String::from(name));
        Ok(())
    }

    /// Find a slot by its registered name.
    pub fn find_by_name(&self, name: &str) -> Option<u32> {
        let inner = self.inner.lock();
        if inner.active_count == 0 {
            return None;
        }
        for (i, slot) in inner.slots.iter().enumerate() {
            if let Some(slot) = slot {
                if slot.metadata.is_valid(&inner.epoch_tracker.current())
                    && slot.name.as_deref() == Some(name)
                {
                    return Some(i as u32);
                }
            }
        }
        None
    }

    /// Return every live slot whose registered name is `name`.
    ///
    /// Names are labels rather than unique keys: a kernel grant may add an
    /// attenuated capability under the same symbolic name.  Authorization
    /// checks that accept any suitable grant must therefore inspect all
    /// matches instead of stopping at the first one.
    pub fn find_all_by_name(&self, name: &str) -> alloc::vec::Vec<u32> {
        let inner = self.inner.lock();
        let current_epoch = inner.epoch_tracker.current();
        inner
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let slot = slot.as_ref()?;
                (slot.metadata.is_valid(&current_epoch) && slot.name.as_deref() == Some(name))
                    .then_some(index as u32)
            })
            .collect()
    }

    /// Revoke a capability by index.
    pub fn revoke(&mut self, index: HandleIndex) -> CapResult<usize> {
        let mut inner = self.inner.lock();
        inner.revoke_slot(index).map(|_| 1)
    }

    /// Revoke every capability by advancing the epoch.
    ///
    /// This is intentionally broader than [`Self::revoke`]: unrelated caps in
    /// this CSpace are invalidated as well.  Slots are reclaimed immediately
    /// so the table cannot slowly fill with unreachable entries.
    pub fn revoke_all_fast(&mut self) -> usize {
        let mut inner = self.inner.lock();
        let mut revoked = 0usize;
        for index in 0..inner.slots.len() {
            if inner.slots[index].is_some() {
                inner.revoke_slot(index as HandleIndex).ok();
                revoked += 1;
            }
        }
        inner.epoch_tracker.revoke();
        revoked
    }

    /// Bump the epoch counter.
    pub fn bump_epoch(&mut self) {
        self.inner.lock().epoch_tracker.revoke();
    }

    /// Snapshot every slot's parent index, returning `None` for free or root
    /// slots.
    pub fn snapshot_parents(&self) -> [Option<HandleIndex>; MAX_CSPACE_SLOTS] {
        let inner = self.inner.lock();
        let mut out = [None; MAX_CSPACE_SLOTS];
        for (i, slot) in inner.slots.iter().enumerate() {
            out[i] = slot
                .as_ref()
                .and_then(|slot| slot.derivation.parent.parent_index());
        }
        out
    }

    /// Get current epoch
    pub fn current_epoch(&self) -> Epoch {
        self.inner.lock().epoch_tracker.current()
    }

    /// Check if a handle is valid
    pub fn is_valid<T>(&self, handle: &Handle<T>) -> bool {
        self.lookup(*handle).is_ok()
    }

    /// Get number of active slots
    pub fn active_count(&self) -> usize {
        self.inner.lock().active_count
    }

    /// Get maximum slots
    pub fn max_slots(&self) -> usize {
        self.inner.lock().max_slots
    }

    /// Snapshot of all active slot indices and generations.
    pub fn snapshot_slots(&self) -> [(Option<HandleIndex>, Option<Generation>); MAX_CSPACE_SLOTS] {
        let inner = self.inner.lock();
        let mut out = [(None, None); MAX_CSPACE_SLOTS];
        for (i, slot) in inner.slots.iter().enumerate() {
            if let Some(slot) = slot {
                if slot.metadata.is_valid(&inner.epoch_tracker.current()) {
                    out[i] = (Some(i as HandleIndex), Some(slot.generation));
                }
            }
        }
        out
    }
}

impl Default for CSpace {
    fn default() -> Self {
        Self::new()
    }
}

impl CSpaceInner {
    pub(crate) fn allocate_slot(
        &mut self,
        permissions: Permissions,
        safety_bits: SafetyBits,
        parent: Option<HandleIndex>,
        guard: Option<Arc<dyn CapGuard>>,
    ) -> CapResult<Handle<u32>> {
        if self.active_count >= self.max_slots {
            return Err(CapError::CSpaceFull);
        }

        if let Some(parent_idx) = parent {
            let parent_slot = self
                .slots
                .get(parent_idx as usize)
                .and_then(Option::as_ref)
                .ok_or(CapError::NotFound)?;
            if !parent_slot.metadata.is_valid(&self.epoch_tracker.current()) {
                return Err(CapError::Revoked);
            }
            if !parent_slot.permissions.contains(perm::DERIVE)
                && parent_slot.permissions != Permissions::all()
            {
                return Err(CapError::Permission);
            }
            if !permissions.is_subset(parent_slot.permissions) {
                return Err(CapError::InvalidDerivation);
            }
            if !crate::safety_policy::SafetyPolicy::validate_safety_bits(
                safety_bits,
                parent_slot.safety_bits,
            ) {
                return Err(CapError::InvalidDerivation);
            }
        }

        let slot_index = self.find_free_slot()?;
        let metadata = SlotMetadata::new(self.epoch_tracker.current());
        let derivation = parent
            .map(DerivationNode::child)
            .unwrap_or_else(DerivationNode::root);
        let hash_key = self.generate_hash_key();
        let generation = Generation(self.generations[slot_index]);

        if slot_index >= self.slots.len() {
            self.slots.resize_with(slot_index + 1, || None);
        }
        self.slots[slot_index] = Some(Box::new(Slot {
            metadata,
            derivation,
            permissions,
            safety_bits,
            hash_key,
            generation,
            guard,
            name: None,
        }));

        self.mark_slot_allocated(slot_index);
        let index = slot_index as HandleIndex;
        self.active_count += 1;

        // The slot is now a durable authorization for the resource.  Record
        // the reference before any fallible derivation-tree update so that
        // `remove_slot` can always release exactly the reference this
        // allocation acquired.  In particular, `link_child` may fail after
        // installing the slot and its rollback path calls `remove_slot`.
        if let Some(guard) = self.slots[slot_index]
            .as_ref()
            .and_then(|slot| slot.guard.as_ref())
        {
            guard.retain();
        }

        if let Some(parent_idx) = parent {
            if let Err(error) = self.link_child(parent_idx) {
                // `link_child` can only fail after the new slot has been
                // installed (on derivation-count overflow), so undo the
                // partial allocation before returning.
                self.remove_slot(index, false).ok();
                return Err(error);
            }
        }

        Ok(Handle::new(index, generation))
    }

    fn link_child(&mut self, parent_idx: HandleIndex) -> CapResult<()> {
        let mut cursor = Some(parent_idx);
        let mut immediate = true;
        while let Some(index) = cursor {
            let slot = self
                .slots
                .get_mut(index as usize)
                .and_then(Option::as_mut)
                .ok_or(CapError::NotFound)?;
            slot.derivation.descendant_count = slot
                .derivation
                .descendant_count
                .checked_add(1)
                .ok_or(CapError::CSpaceFull)?;
            if immediate {
                slot.derivation.child_count = slot
                    .derivation
                    .child_count
                    .checked_add(1)
                    .ok_or(CapError::CSpaceFull)?;
                immediate = false;
            }
            cursor = slot.derivation.parent.parent_index();
        }
        Ok(())
    }

    fn unlink_child(&mut self, parent_idx: HandleIndex) {
        let mut cursor = Some(parent_idx);
        let mut immediate = true;
        let mut visited = [false; MAX_CSPACE_SLOTS];
        while let Some(index) = cursor {
            let slot_index = index as usize;
            if slot_index >= MAX_CSPACE_SLOTS || visited[slot_index] {
                break;
            }
            visited[slot_index] = true;
            let Some(Some(slot)) = self.slots.get_mut(slot_index) else {
                break;
            };
            slot.derivation.descendant_count = slot.derivation.descendant_count.saturating_sub(1);
            if immediate {
                slot.derivation.child_count = slot.derivation.child_count.saturating_sub(1);
                immediate = false;
            }
            cursor = slot.derivation.parent.parent_index();
        }
    }

    pub(crate) fn retain_slot(&mut self, index: HandleIndex) -> Option<Generation> {
        let slot = self.slots.get_mut(index as usize)?.as_mut()?;
        if !slot.metadata.is_valid(&self.epoch_tracker.current()) {
            return None;
        }
        slot.metadata.inc_ref();
        Some(slot.generation)
    }

    pub(crate) fn release_slot(&mut self, index: HandleIndex, generation: Generation) {
        let should_free = match self.slots.get_mut(index as usize).and_then(Option::as_mut) {
            Some(slot) if slot.generation == generation => slot.metadata.dec_ref() == 0,
            _ => false,
        };
        if should_free {
            // Ordinary `Cap` destruction releases this slot's authorization.
            // It must not globally invalidate a resource that is still
            // reachable through another slot (for example a granted slot in
            // a different CSpace).  `ResourceCell::release` invalidates when
            // the final authorization disappears.
            self.remove_slot(index, false).ok();
        }
    }

    fn revoke_slot(&mut self, index: HandleIndex) -> CapResult<()> {
        self.remove_slot(index, true)
    }

    fn remove_slot(&mut self, index: HandleIndex, invalidate: bool) -> CapResult<()> {
        let slot_index = index as usize;
        if slot_index >= MAX_CSPACE_SLOTS {
            return Err(CapError::NotFound);
        }
        let (parent, guard) = {
            let slot = self
                .slots
                .get_mut(slot_index)
                .and_then(Option::as_mut)
                .ok_or(CapError::NotFound)?;
            if !slot.metadata.is_valid(&self.epoch_tracker.current()) {
                return Err(CapError::Revoked);
            }
            slot.metadata.invalidate();
            slot.metadata.ref_count = 0;
            (slot.derivation.parent.parent_index(), slot.guard.take())
        };

        if let Some(parent) = parent {
            self.unlink_child(parent);
        }

        if let Some(guard) = guard {
            if invalidate {
                guard.invalidate();
            }
            guard.release();
        }

        if let Some(slot) = self.slots.get_mut(slot_index) {
            *slot = None;
        }
        self.active_count = self.active_count.saturating_sub(1);
        self.mark_slot_free(slot_index);
        self.generations[slot_index] = self.generations[slot_index].wrapping_add(1);
        Ok(())
    }

    fn lookup_slot(&self, index: HandleIndex, generation: Generation) -> CapResult<&Slot> {
        let slot = match self.slots.get(index as usize).and_then(Option::as_ref) {
            Some(slot) => slot,
            None => {
                return match self.generations.get(index as usize) {
                    Some(current) if *current != generation.0 => Err(CapError::Revoked),
                    _ => Err(CapError::NotFound),
                };
            }
        };
        if slot.generation != generation {
            return Err(CapError::Revoked);
        }
        if !slot.metadata.is_valid(&self.epoch_tracker.current()) {
            return Err(CapError::Revoked);
        }
        Ok(slot)
    }

    fn find_free_slot(&self) -> CapResult<usize> {
        for (bitmap_idx, &bitmap) in self.free_bitmap.iter().enumerate() {
            if bitmap != 0 {
                let bit_pos = bitmap.trailing_zeros() as usize;
                let slot_index = bitmap_idx * 64 + bit_pos;
                if slot_index < self.max_slots {
                    return Ok(slot_index);
                }
            }
        }
        Err(CapError::OutOfSlots)
    }

    fn mark_slot_allocated(&mut self, index: usize) {
        self.free_bitmap[index / 64] &= !(1u64 << (index % 64));
    }

    fn mark_slot_free(&mut self, index: usize) {
        self.free_bitmap[index / 64] |= 1u64 << (index % 64);
    }

    fn generate_hash_key(&self) -> u64 {
        static COUNTER_HI: AtomicU32 = AtomicU32::new(0);
        static COUNTER_LO: AtomicU32 = AtomicU32::new(1);
        let lo = COUNTER_LO.fetch_add(1, Ordering::Relaxed);
        let hi = if lo == 0 {
            COUNTER_HI.fetch_add(1, Ordering::Relaxed)
        } else {
            0
        };
        ((hi as u64) << 32) | lo as u64
    }
}

/// A slot in the CSpace
pub struct Slot {
    /// Metadata including epoch and ref count
    pub metadata: SlotMetadata,
    /// Derivation tree node
    pub derivation: DerivationNode,
    /// Permissions for this capability
    pub permissions: Permissions,
    /// Safety bits
    pub safety_bits: SafetyBits,
    /// Hash key for the internal index
    pub hash_key: u64,
    /// Generation embedded in handles for this occupancy of the slot.
    pub generation: Generation,
    /// Revocation guard shared with typed capabilities.
    pub guard: Option<Arc<dyn CapGuard>>,
    /// Optional discoverable name.
    pub name: Option<String>,
}

/// Immutable snapshot of a live slot.
#[derive(Debug, Clone)]
pub struct SlotSnapshot {
    /// Epoch and reference-count metadata.
    pub metadata: SlotMetadata,
    /// Derivation links and counts.
    pub derivation: DerivationNode,
    /// Permission bits.
    pub permissions: Permissions,
    /// Safety bits.
    pub safety_bits: SafetyBits,
    /// Handle generation.
    pub generation: Generation,
    /// Optional discoverable name.
    pub name: Option<String>,
}

impl From<&Slot> for SlotSnapshot {
    fn from(slot: &Slot) -> Self {
        Self {
            metadata: slot.metadata,
            derivation: slot.derivation,
            permissions: slot.permissions,
            safety_bits: slot.safety_bits,
            generation: slot.generation,
            name: slot.name.clone(),
        }
    }
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            metadata: SlotMetadata::default(),
            derivation: DerivationNode::default(),
            permissions: Permissions::empty(),
            safety_bits: SafetyBits::default(),
            hash_key: 0,
            generation: Generation::default(),
            guard: None,
            name: None,
        }
    }
}

/// Weak reference to a CSpace table, used by capabilities to release their
/// slot without borrowing the owning process.
pub(crate) type WeakCSpace = Weak<SpinLock<CSpaceInner>>;

/// Counter used for detached capability handles.
pub(crate) fn next_detached_handle() -> HandleIndex {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_parent() {
        let mut cspace = CSpace::new();
        assert_eq!(
            cspace.allocate(Permissions::all(), SafetyBits::default(), Some(99)),
            Err(CapError::NotFound)
        );
    }

    #[test]
    fn rejects_child_without_parent_derive_right() {
        let mut cspace = CSpace::new();
        let parent = cspace
            .allocate(perm::READ, SafetyBits::default(), None)
            .unwrap();
        assert_eq!(
            cspace.allocate(perm::READ, SafetyBits::default(), Some(parent.index())),
            Err(CapError::Permission)
        );
    }

    #[test]
    fn rejects_permission_amplification() {
        let mut cspace = CSpace::new();
        let parent = cspace
            .allocate(perm::READ | perm::DERIVE, SafetyBits::default(), None)
            .unwrap();
        assert_eq!(
            cspace.allocate(
                perm::READ | perm::WRITE | perm::DERIVE,
                SafetyBits::default(),
                Some(parent.index())
            ),
            Err(CapError::InvalidDerivation)
        );
    }

    #[test]
    fn rejects_safety_bit_amplification() {
        let mut cspace = CSpace::new();
        let parent = cspace
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        let mut amplified = SafetyBits::default();
        amplified.set_cross_namespace(true);
        assert_eq!(
            cspace.allocate(perm::READ, amplified, Some(parent.index())),
            Err(CapError::InvalidDerivation)
        );
    }

    #[test]
    fn derivation_counts_include_all_ancestors() {
        let mut cspace = CSpace::new();
        let root = cspace
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        let child = cspace
            .allocate(
                perm::READ | perm::DERIVE,
                SafetyBits::default(),
                Some(root.index()),
            )
            .unwrap();
        let grandchild = cspace
            .allocate(perm::READ, SafetyBits::default(), Some(child.index()))
            .unwrap();

        let root_slot = cspace.lookup(root).unwrap();
        assert_eq!(root_slot.derivation.child_count(), 1);
        assert_eq!(root_slot.derivation.descendant_count(), 2);
        let child_slot = cspace.lookup(child).unwrap();
        assert_eq!(child_slot.derivation.child_count(), 1);
        assert_eq!(child_slot.derivation.descendant_count(), 1);
        assert!(cspace.lookup(grandchild).is_ok());
    }

    #[test]
    fn revoking_child_updates_ancestor_counts() {
        let mut cspace = CSpace::new();
        let root = cspace
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        let child = cspace
            .allocate(perm::READ, SafetyBits::default(), Some(root.index()))
            .unwrap();
        cspace.revoke(child.index()).unwrap();
        let root_slot = cspace.lookup(root).unwrap();
        assert_eq!(root_slot.derivation.child_count(), 0);
        assert_eq!(root_slot.derivation.descendant_count(), 0);
    }

    #[test]
    fn slot_reuse_rejects_stale_generation() {
        let mut cspace = CSpace::new();
        let first = cspace
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        let old = Handle::<u32>::new(first.index(), first.generation());
        cspace.revoke(first.index()).unwrap();
        let second = cspace
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        assert_eq!(first.index(), second.index());
        assert_ne!(first.generation(), second.generation());
        assert!(matches!(cspace.lookup(old), Err(CapError::Revoked)));
        assert!(cspace.lookup(second).is_ok());
    }

    #[test]
    fn named_slot_can_be_found_and_is_removed_on_revoke() {
        let mut cspace = CSpace::new();
        let handle = cspace
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        cspace.bind_name(handle.index(), "uart0").unwrap();
        assert_eq!(cspace.find_by_name("uart0"), Some(handle.index()));
        cspace.revoke(handle.index()).unwrap();
        assert_eq!(cspace.find_by_name("uart0"), None);
    }

    #[test]
    fn capacity_limit_is_exact() {
        let mut cspace = CSpace::with_max_slots(2);
        assert!(cspace
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .is_ok());
        assert!(cspace
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .is_ok());
        assert_eq!(
            cspace.allocate(Permissions::all(), SafetyBits::default(), None),
            Err(CapError::CSpaceFull)
        );
    }

    #[test]
    fn fast_revoke_reclaims_all_slots() {
        let mut cspace = CSpace::new();
        let handles: [Handle<u32>; 3] = core::array::from_fn(|_| {
            cspace
                .allocate(Permissions::all(), SafetyBits::default(), None)
                .unwrap()
        });
        assert_eq!(cspace.revoke_all_fast(), 3);
        assert_eq!(cspace.active_count(), 0);
        for handle in handles {
            assert!(matches!(cspace.lookup(handle), Err(CapError::Revoked)));
        }
    }

    #[test]
    fn repeated_revoke_is_rejected() {
        let mut cspace = CSpace::new();
        let handle = cspace
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        assert!(cspace.revoke(handle.index()).is_ok());
        assert_eq!(cspace.revoke(handle.index()), Err(CapError::NotFound));
    }

    #[test]
    fn clone_slot_into_copies_metadata_and_name() {
        let mut source = CSpace::new();
        let mut target = CSpace::new();
        let source_handle = source
            .allocate(perm::READ | perm::TRANSFER, SafetyBits::default(), None)
            .unwrap();
        source.bind_name(source_handle.index(), "uart0").unwrap();

        let copied = source
            .clone_slot_into(source_handle.index(), &mut target)
            .unwrap();
        let snapshot = target.lookup(copied).unwrap();
        assert_eq!(snapshot.permissions, perm::READ | perm::TRANSFER);
        assert_eq!(target.find_by_name("uart0"), Some(copied.index()));
        // The source remains valid and is not rewritten by the copy.
        assert!(source.lookup(source_handle).is_ok());
    }

    #[test]
    fn clone_slot_into_rejects_stale_source_and_same_cspace() {
        let mut source = CSpace::new();
        let mut target = CSpace::new();
        let handle = source
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        source.revoke(handle.index()).unwrap();
        assert_eq!(
            source.clone_slot_into(handle.index(), &mut target),
            Err(CapError::NotFound)
        );

        let live = source
            .allocate(Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        let mut same = source.clone();
        assert_eq!(
            source.clone_slot_into(live.index(), &mut same),
            Err(CapError::Invalid)
        );
    }

    #[test]
    fn granted_slot_outlives_source_cap() {
        let mut source = CSpace::new();
        let mut target = CSpace::new();
        let cap = source
            .insert_cap(7u32, Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        let granted = source
            .clone_slot_into(cap.handle().index(), &mut target)
            .unwrap();

        let target_guard = target.inner.lock().slots[granted.index() as usize]
            .as_ref()
            .and_then(|slot| slot.guard.clone())
            .expect("granted slot guard");

        // Dropping the last typed Cap removes the source authorization but
        // must not invalidate the independently granted destination slot.
        drop(cap);
        assert!(target_guard.is_valid());
        assert!(target.lookup(granted).is_ok());

        // Explicit revocation of the granted slot still revokes the shared
        // resource and balances the slot reference exactly once.
        target.revoke(granted.index()).unwrap();
        assert!(!target_guard.is_valid());
    }

    #[test]
    fn failed_grant_does_not_leak_a_slot_reference() {
        let mut source = CSpace::new();
        let mut full = CSpace::with_max_slots(0);
        let cap = source
            .insert_cap(9u32, Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        let guard = source.inner.lock().slots[cap.handle().index() as usize]
            .as_ref()
            .and_then(|slot| slot.guard.clone())
            .expect("source guard");

        assert_eq!(
            source.clone_slot_into(cap.handle().index(), &mut full),
            Err(CapError::CSpaceFull)
        );
        drop(cap);
        assert!(
            !guard.is_valid(),
            "a failed grant must not leave an authorization reference behind"
        );
    }

    #[test]
    fn failed_derivation_link_rolls_back_slot_reference() {
        let mut cspace = CSpace::new();
        let cap = cspace
            .insert_cap(13u32, Permissions::all(), SafetyBits::default(), None)
            .unwrap();
        let guard = cspace.inner.lock().slots[cap.handle().index() as usize]
            .as_ref()
            .and_then(|slot| slot.guard.clone())
            .expect("source guard");

        // Force the descendant counter to overflow after the new slot has
        // been installed but while `link_child` is still fallible.
        cspace.inner.lock().slots[cap.handle().index() as usize]
            .as_mut()
            .unwrap()
            .derivation
            .descendant_count = u32::MAX;

        assert!(matches!(
            cspace.derive_cap(&cap, Permissions::all(), SafetyBits::default()),
            Err(CapError::CSpaceFull)
        ));
        assert_eq!(cspace.active_count(), 1);

        drop(cap);
        assert!(
            !guard.is_valid(),
            "a failed derivation must not leave a slot authorization behind"
        );
    }
}
