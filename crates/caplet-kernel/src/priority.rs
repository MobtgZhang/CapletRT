//! Priority definitions

use core::cmp::Ordering;

/// Task priority (0 = highest, 31 = lowest)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Priority(pub u8);

impl Priority {
    /// Highest priority
    pub const HIGHEST: Self = Self(0);
    /// Lowest priority
    pub const LOWEST: Self = Self(31);
    /// Default user priority
    pub const DEFAULT_USER: Self = Self(16);
    /// Idle task priority
    pub const IDLE: Self = Self(31);

    /// Create a new priority
    pub fn new(p: u8) -> Self {
        Self(p.min(31))
    }

    /// Check if this is the highest priority
    pub fn is_highest(&self) -> bool {
        self.0 == 0
    }

    /// Check if this is the lowest priority
    pub fn is_lowest(&self) -> bool {
        self.0 == 31
    }
}

impl core::cmp::Ord for Priority {
    fn cmp(&self, other: &Self) -> Ordering {
        // Lower number = higher priority
        other.0.cmp(&self.0)
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Default for Priority {
    fn default() -> Self {
        Self::DEFAULT_USER
    }
}

impl From<u8> for Priority {
    fn from(p: u8) -> Self {
        Self::new(p)
    }
}

impl From<Priority> for u8 {
    fn from(p: Priority) -> Self {
        p.0
    }
}