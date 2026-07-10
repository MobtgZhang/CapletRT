//! CSpace slot allocator
//!
//! Uses a bitmap for O(1) allocate and free operations.

use crate::kernel_config::KernelConfig;

/// Slot allocator for CSpace
pub struct SlotAllocator {
    /// Bitmap tracking which slots are allocated (1 = allocated, 0 = free)
    /// Using u64 words for efficient bit operations
    bitmap: u64,
    /// Number of bits currently set (allocated slots)
    allocated_count: usize,
    /// Maximum number of slots
    max_slots: usize,
}

impl SlotAllocator {
    /// Create a new slot allocator
    pub const fn new(max_slots: usize) -> Self {
        // Cap at 64 slots for simple bitmap implementation
        // For larger CSpaces, this can be extended with a Vec<u64>
        Self {
            bitmap: 0,
            allocated_count: 0,
            max_slots: if max_slots > 64 { 64 } else { max_slots },
        }
    }

    /// Allocate a slot - O(1) using bitmap
    pub fn allocate(&mut self) -> Option<usize> {
        if self.allocated_count >= self.max_slots {
            return None;
        }

        // Find first zero bit (free slot)
        let slot = self.bitmap.trailing_ones() as usize;

        if slot >= self.max_slots {
            return None;
        }

        // Mark slot as allocated
        self.bitmap |= 1 << slot;
        self.allocated_count += 1;

        Some(slot)
    }

    /// Free a slot - O(1) using bitmap
    pub fn free(&mut self, slot: usize) {
        if slot >= self.max_slots {
            return;
        }

        let mask = 1u64 << slot;
        if self.bitmap & mask != 0 {
            self.bitmap &= !mask;
            self.allocated_count -= 1;
        }
    }

    /// Get number of allocated slots
    pub fn allocated(&self) -> usize {
        self.allocated_count
    }

    /// Get maximum number of slots
    pub fn max_slots(&self) -> usize {
        self.max_slots
    }

    /// Check if a slot is allocated
    pub fn is_valid(&self, slot: usize) -> bool {
        slot < self.max_slots && (self.bitmap & (1 << slot)) != 0
    }

    /// Get free slots remaining
    pub fn free_slots(&self) -> usize {
        self.max_slots - self.allocated_count
    }

    /// Check if allocator is full
    pub fn is_full(&self) -> bool {
        self.allocated_count >= self.max_slots
    }

    /// Check if allocator is empty
    pub fn is_empty(&self) -> bool {
        self.allocated_count == 0
    }

    /// Iterate over all allocated slots
    pub fn allocated_slots(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.max_slots).filter(move |&s| (self.bitmap & (1 << s)) != 0)
    }
}

impl Default for SlotAllocator {
    fn default() -> Self {
        Self::new(256)
    }
}

/// Initialize slot allocator from config
pub fn init_slot_allocator(config: &KernelConfig) -> SlotAllocator {
    let max = config.max_tasks * 4 + 32; // Estimate based on task count
    SlotAllocator::new(max)
}
