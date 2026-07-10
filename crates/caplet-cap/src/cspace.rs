//! CSpace - Capability Space implementation

use crate::error::{CapError, CapResult};
use crate::epoch::{Epoch, EpochTracker, SlotMetadata};
use crate::handle::{Handle, HandleIndex, Generation};
use crate::parent_link::DerivationNode;
use crate::permissions::Permissions;
use crate::safety_bits::SafetyBits;
use alloc::format;
use hashbrown::HashMap;

/// Maximum number of slots in a CSpace
pub const MAX_CSPACE_SLOTS: usize = 256;

/// Bitmask size for fast-path slot lookup
pub const SLOT_BITMAP_SIZE: usize = MAX_CSPACE_SLOTS / 64;

/// CSpace - the capability space for a task
///
/// Each task has its own CSpace that stores capabilities in slots.
/// CSpace provides both fast-path (bitmask) and slow-path (hash) lookup.
pub struct CSpace {
    /// Slot table - maps index to slot data
    slots: [Option<Slot>; MAX_CSPACE_SLOTS],
    /// Fast-path free slot bitmask
    free_bitmap: [u64; SLOT_BITMAP_SIZE],
    /// Slow-path hash index
    hash_index: HashMap<u64, HandleIndex>,
    /// Epoch tracker for fast revocation
    epoch_tracker: EpochTracker,
    /// Total active slots
    active_count: usize,
    /// Maximum slots allowed
    max_slots: usize,
}

impl CSpace {
    /// Create a new empty CSpace
    pub fn new() -> Self {
        Self {
            slots: [const { None }; MAX_CSPACE_SLOTS],
            free_bitmap: [u64::MAX; SLOT_BITMAP_SIZE],
            hash_index: HashMap::new(),
            epoch_tracker: EpochTracker::new(),
            active_count: 0,
            max_slots: MAX_CSPACE_SLOTS,
        }
    }

    /// Create a CSpace with a custom maximum slot count
    pub fn with_max_slots(max: usize) -> Self {
        let clamped_max = max.min(MAX_CSPACE_SLOTS);
        Self {
            slots: [const { None }; MAX_CSPACE_SLOTS],
            free_bitmap: [u64::MAX; SLOT_BITMAP_SIZE],
            hash_index: HashMap::new(),
            epoch_tracker: EpochTracker::new(),
            active_count: 0,
            max_slots: clamped_max,
        }
    }

    /// Allocate a new slot
    pub fn allocate(
        &mut self,
        permissions: Permissions,
        safety_bits: SafetyBits,
        parent: Option<HandleIndex>,
    ) -> CapResult<Handle<u32>> {
        if self.active_count >= self.max_slots {
            return Err(CapError::CSpaceFull);
        }

        let slot_index = self.find_free_slot()?;

        let metadata = SlotMetadata::new(self.epoch_tracker.current());
        let derivation = if let Some(parent_idx) = parent {
            DerivationNode::child(parent_idx)
        } else {
            DerivationNode::root()
        };

        let hash_key = self.generate_hash_key();

        self.slots[slot_index] = Some(Slot {
            metadata,
            derivation,
            permissions,
            safety_bits,
            hash_key,
        });

        self.mark_slot_allocated(slot_index);
        let slot_index_u32 = slot_index as HandleIndex;
        self.hash_index.insert(hash_key, slot_index_u32);

        self.active_count += 1;

        Ok(Handle::new(slot_index_u32, Generation::default()))
    }

    /// Look up a capability by handle
    pub fn lookup<T>(&self, handle: Handle<T>) -> CapResult<&Slot> {
        let slot = self.slots.get(handle.index() as usize).ok_or(CapError::NotFound)?;

        match slot {
            Some(s) => {
                if !s.metadata.is_valid(&self.epoch_tracker.current()) {
                    return Err(CapError::Revoked);
                }
                Ok(s)
            }
            None => Err(CapError::NotFound),
        }
    }

    /// Look up a capability by handle (mutable)
    pub fn lookup_mut<T>(&mut self, handle: Handle<T>) -> CapResult<&mut Slot> {
        let slot = self.slots.get_mut(handle.index() as usize).ok_or(CapError::NotFound)?;

        match slot {
            Some(s) => {
                if !s.metadata.is_valid(&self.epoch_tracker.current()) {
                    return Err(CapError::Revoked);
                }
                Ok(s)
            }
            None => Err(CapError::NotFound),
        }
    }

    /// Read a slot's permissions and safety bits by raw index. Used by
    /// the syscall layer where we have a `u32` index rather than a typed
    /// `Handle<T>`.
    pub fn slot_perms_safety(
        &self,
        index: u32,
    ) -> Option<(Permissions, SafetyBits)> {
        let slot = self.slots.get(index as usize)?.as_ref()?;
        if !slot.metadata.is_valid(&self.epoch_tracker.current()) {
            return None;
        }
        Some((slot.permissions, slot.safety_bits))
    }

    /// Find a slot by name (string match against the slot's `hash_key`
    /// *or* a path-key the kernel uses for service discovery).
    pub fn find_by_name(&self, name: &str) -> Option<u32> {
        for (i, s) in self.slots.iter().enumerate() {
            if let Some(slot) = s {
                if !slot.metadata.is_valid(&self.epoch_tracker.current()) {
                    continue;
                }
                // We use the slot's `hash_key` as a stable identifier;
                // the service-discovery mapping is done at the kernel
                // level by storing a `CapKey` next to it. Here we
                // expose the index for direct lookup.
                let key = format!("cap:{}", i);
                if key == name {
                    return Some(i as u32);
                }
            }
        }
        None
    }

    /// Revoke a capability by index
    pub fn revoke(&mut self, index: HandleIndex) -> CapResult<usize> {
        if (index as usize) >= MAX_CSPACE_SLOTS {
            return Err(CapError::NotFound);
        }

        let to_revoke;
        let hash_key;
        {
            let slot = self.slots.get_mut(index as usize).ok_or(CapError::NotFound)?;
            match slot {
                Some(s) => {
                    to_revoke = s.derivation.descendant_count() + 1;
                    s.metadata.invalidate();
                    hash_key = s.hash_key;
                }
                None => return Err(CapError::NotFound),
            }
        }

        self.active_count -= 1;
        self.mark_slot_free(index as usize);
        self.hash_index.remove(&hash_key);

        Ok(to_revoke as usize)
    }

    /// Bump the epoch counter. After this returns, all handles whose
    /// `metadata.allocation_epoch != current_epoch` will be considered
    /// revoked by `lookup`. Used by the fast revocation path.
    pub fn bump_epoch(&mut self) {
        self.epoch_tracker.revoke();
    }

    /// Snapshot every slot's parent index, returning `None` for free
    /// or root slots. Used by the slow revocation path to walk the
    /// derivation tree without holding a mutable borrow across the
    /// walk.
    pub fn snapshot_parents(&self) -> [Option<HandleIndex>; MAX_CSPACE_SLOTS] {
        let mut out = [None; MAX_CSPACE_SLOTS];
        for (i, s) in self.slots.iter().enumerate() {
            out[i] = s.as_ref().and_then(|s| s.derivation.parent.parent_index());
        }
        out
    }

    /// Get current epoch
    pub fn current_epoch(&self) -> Epoch {
        self.epoch_tracker.current()
    }

    /// Check if a handle is valid
    pub fn is_valid<T>(&self, handle: &Handle<T>) -> bool {
        self.lookup(*handle).is_ok()
    }

    /// Get number of active slots
    pub fn active_count(&self) -> usize {
        self.active_count
    }

    /// Get maximum slots
    pub fn max_slots(&self) -> usize {
        self.max_slots
    }

    /// Find a free slot using bitmap
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

    /// Mark a slot as allocated in bitmap
    fn mark_slot_allocated(&mut self, index: usize) {
        let bitmap_idx = index / 64;
        let bit_pos = index % 64;
        self.free_bitmap[bitmap_idx] &= !(1u64 << bit_pos);
    }

    /// Mark a slot as free in bitmap
    fn mark_slot_free(&mut self, index: usize) {
        let bitmap_idx = index / 64;
        let bit_pos = index % 64;
        self.free_bitmap[bitmap_idx] |= 1u64 << bit_pos;
    }

    /// Generate a unique hash key for a slot
    fn generate_hash_key(&self) -> u64 {
        // Use a spinlock-protected counter so we work on targets that
        // lack `AtomicU64` (e.g. ARMv6-M).
        use core::sync::atomic::{AtomicU32, Ordering};
        static COUNTER_HI: AtomicU32 = AtomicU32::new(0);
        static COUNTER_LO: AtomicU32 = AtomicU32::new(1);
        let lo = COUNTER_LO.fetch_add(1, Ordering::Relaxed);
        let hi = if lo == 0 { COUNTER_HI.fetch_add(1, Ordering::Relaxed) } else { 0 };
        ((hi as u64) << 32) | (lo as u64)
    }
}

impl Default for CSpace {
    fn default() -> Self {
        Self::new()
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
    /// Hash key for slow-path lookup
    pub hash_key: u64,
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            metadata: SlotMetadata::default(),
            derivation: DerivationNode::default(),
            permissions: Permissions::empty(),
            safety_bits: SafetyBits::default(),
            hash_key: 0,
        }
    }
}