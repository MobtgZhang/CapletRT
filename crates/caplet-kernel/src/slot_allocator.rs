//! CSpace slot allocator
//!
//! Uses a bitmap for O(1) allocate and free operations.

use crate::kernel_config::KernelConfig;

/// Slot allocator for CSpace
pub struct SlotAllocator {
    /// Bitmap tracking which slots are allocated (1 = allocated, 0 = free).
    /// Four words cover the full 256-slot CSpace; the previous scalar
    /// silently clamped every larger request to 64.
    bitmap: [u64; Self::WORDS],
    /// Number of bits currently set (allocated slots)
    allocated_count: usize,
    /// Maximum number of slots
    max_slots: usize,
}

impl SlotAllocator {
    /// Number of 64-bit words needed for the maximum CSpace size.
    pub const WORDS: usize = 4;
    /// Maximum supported slots (`WORDS * 64`).
    pub const CAPACITY: usize = Self::WORDS * 64;

    /// Create a new slot allocator
    pub const fn new(max_slots: usize) -> Self {
        Self {
            bitmap: [0; Self::WORDS],
            allocated_count: 0,
            max_slots: if max_slots > Self::CAPACITY {
                Self::CAPACITY
            } else {
                max_slots
            },
        }
    }

    /// Allocate a slot - O(1) using bitmap
    pub fn allocate(&mut self) -> Option<usize> {
        if self.allocated_count >= self.max_slots {
            return None;
        }

        // Find the first zero bit (free slot) across all words.
        for (word_index, word) in self.bitmap.iter_mut().enumerate() {
            if *word != u64::MAX {
                let bit = word.trailing_ones() as usize;
                let slot = word_index * 64 + bit;
                if slot >= self.max_slots {
                    return None;
                }
                *word |= 1u64 << bit;
                self.allocated_count += 1;
                return Some(slot);
            }
        }
        None
    }

    /// Free a slot - O(1) using bitmap
    pub fn free(&mut self, slot: usize) {
        if slot >= self.max_slots {
            return;
        }

        let word = slot / 64;
        let mask = 1u64 << (slot % 64);
        if self.bitmap[word] & mask != 0 {
            self.bitmap[word] &= !mask;
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
        slot < self.max_slots && (self.bitmap[slot / 64] & (1u64 << (slot % 64))) != 0
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
        (0..self.max_slots).filter(move |&s| self.is_valid(s))
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
