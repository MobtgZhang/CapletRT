//! Epoch-based fast path for revocation

/// Epoch counter for fast-path revocation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Epoch(u64);

impl Epoch {
    /// Create a new epoch starting at 0
    pub const fn new() -> Self {
        Self(0)
    }

    /// Get the current epoch value
    pub const fn value(&self) -> u64 {
        self.0
    }

    /// Increment the epoch
    pub fn increment(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }

    /// Check if this epoch matches the current epoch
    pub fn matches(&self, current: &Epoch) -> bool {
        self.0 == current.0
    }

    /// Create an epoch from a u64 value
    pub const fn from_u64(value: u64) -> Self {
        Self(value)
    }
}

/// Epoch tracker for managing revocation state
pub struct EpochTracker {
    current: Epoch,
    retired_count: usize,
}

impl EpochTracker {
    /// Create a new epoch tracker
    pub const fn new() -> Self {
        Self {
            current: Epoch::new(),
            retired_count: 0,
        }
    }

    /// Get the current epoch
    pub const fn current(&self) -> Epoch {
        self.current
    }

    /// Revoke by incrementing epoch
    pub fn revoke(&mut self) {
        self.current.increment();
    }

    /// Get the number of retired capabilities
    pub const fn retired_count(&self) -> usize {
        self.retired_count
    }

    /// Increment retired count
    pub fn increment_retired(&mut self) {
        self.retired_count += 1;
    }

    /// Reset retired count
    pub fn reset_retired(&mut self) {
        self.retired_count = 0;
    }

    /// Validate a capability against current epoch
    pub fn validate(&self, creation_epoch: Epoch) -> bool {
        creation_epoch.matches(&self.current)
    }
}

impl Default for EpochTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Slot metadata including epoch information
#[derive(Debug, Clone, Copy)]
pub struct SlotMetadata {
    /// Epoch at which this slot was allocated
    pub allocation_epoch: Epoch,
    /// Whether this slot is currently valid
    pub valid: bool,
    /// Reference count
    pub ref_count: u32,
}

impl SlotMetadata {
    /// Create new metadata for an allocated slot
    pub const fn new(epoch: Epoch) -> Self {
        Self {
            allocation_epoch: epoch,
            valid: true,
            ref_count: 1,
        }
    }

    /// Check if the slot is valid for the given epoch
    pub fn is_valid(&self, current_epoch: &Epoch) -> bool {
        self.valid && self.allocation_epoch.matches(current_epoch)
    }

    /// Invalidate the slot
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Increment reference count
    pub fn inc_ref(&mut self) {
        self.ref_count = self.ref_count.saturating_add(1);
    }

    /// Decrement reference count
    pub fn dec_ref(&mut self) -> u32 {
        if self.ref_count > 0 {
            self.ref_count -= 1;
        }
        self.ref_count
    }

    /// Get reference count
    pub const fn ref_count(&self) -> u32 {
        self.ref_count
    }

    /// Get allocation epoch
    pub const fn allocation_epoch(&self) -> Epoch {
        self.allocation_epoch
    }
}

impl Default for SlotMetadata {
    fn default() -> Self {
        Self {
            allocation_epoch: Epoch::new(),
            valid: false,
            ref_count: 0,
        }
    }
}
