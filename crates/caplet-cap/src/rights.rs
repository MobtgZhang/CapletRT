//! Rights checking utilities

use super::permissions::{self, Permissions};

/// Rights filter for capability derivation
#[derive(Debug, Clone, Copy, Default)]
pub struct RightsFilter {
    /// Minimum permissions to retain
    pub min_required: Permissions,
    /// Maximum permissions allowed
    pub max_allowed: Permissions,
}

/// Type alias - RightsFilter is the main rights type
pub type Rights = RightsFilter;

impl RightsFilter {
    /// Create a new rights filter with given bounds
    pub const fn new(min_required: Permissions, max_allowed: Permissions) -> Self {
        Self {
            min_required,
            max_allowed,
        }
    }

    /// Create a filter that only allows the specified permissions
    pub const fn only(perms: Permissions) -> Self {
        Self {
            min_required: perms,
            max_allowed: perms,
        }
    }

    /// Create a filter that allows all permissions up to the given limit
    pub const fn up_to(max_allowed: Permissions) -> Self {
        Self {
            min_required: Permissions::empty(),
            max_allowed,
        }
    }

    /// Apply this filter to parent permissions
    pub fn apply(&self, parent: Permissions) -> Permissions {
        let allowed = parent & self.max_allowed;
        if parent.contains(self.min_required) {
            allowed | self.min_required
        } else {
            allowed
        }
    }

    /// Check if this filter would change the permissions
    pub fn would_change(&self, parent: Permissions) -> bool {
        self.apply(parent) != parent
    }
}

/// Memory region rights for address space operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRights(pub u8);

impl MemoryRights {
    /// Read permission
    pub const READ: u8 = 1 << 0;
    /// Write permission
    pub const WRITE: u8 = 1 << 1;
    /// Execute permission
    pub const EXECUTE: u8 = 1 << 2;

    /// Create from raw bits
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Get raw bits
    pub const fn bits(&self) -> u8 {
        self.0
    }

    /// Check if readable
    pub const fn is_readable(&self) -> bool {
        (self.0 & Self::READ) != 0
    }

    /// Check if writable
    pub const fn is_writable(&self) -> bool {
        (self.0 & Self::WRITE) != 0
    }

    /// Check if executable
    pub const fn is_executable(&self) -> bool {
        (self.0 & Self::EXECUTE) != 0
    }

    /// Create read-only rights
    pub const fn read_only() -> Self {
        Self(Self::READ)
    }

    /// Create read-write rights
    pub const fn read_write() -> Self {
        Self(Self::READ | Self::WRITE)
    }

    /// Create read-execute rights
    pub const fn read_execute() -> Self {
        Self(Self::READ | Self::EXECUTE)
    }

    /// Create read-write-execute rights
    pub const fn read_write_execute() -> Self {
        Self(Self::READ | Self::WRITE | Self::EXECUTE)
    }
}

impl Default for MemoryRights {
    fn default() -> Self {
        Self::read_write()
    }
}

impl From<MemoryRights> for Permissions {
    fn from(rights: MemoryRights) -> Self {
        let mut perms = Permissions::empty();
        if rights.is_readable() {
            perms |= permissions::perm::READ;
        }
        if rights.is_writable() {
            perms |= permissions::perm::WRITE;
        }
        if rights.is_executable() {
            perms |= permissions::perm::EXECUTE;
        }
        perms
    }
}
