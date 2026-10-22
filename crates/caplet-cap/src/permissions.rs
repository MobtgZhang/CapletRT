//! Permission types for capabilities

/// Permission flags for capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Permissions(pub u32);

/// Individual permission bits
pub mod perm {
    use super::Permissions;

    /// Read permission
    pub const READ: Permissions = Permissions(1 << 0);
    /// Write permission
    pub const WRITE: Permissions = Permissions(1 << 1);
    /// Execute permission
    pub const EXECUTE: Permissions = Permissions(1 << 2);
    /// Owner permission (can grant/revoke)
    pub const OWNER: Permissions = Permissions(1 << 3);
    /// Create child capabilities
    pub const DERIVE: Permissions = Permissions(1 << 4);
    /// Transfer to another task
    pub const TRANSFER: Permissions = Permissions(1 << 5);
    /// Delete/revoke capability
    pub const DELETE: Permissions = Permissions(1 << 6);
    /// Map into address space
    pub const MAP: Permissions = Permissions(1 << 7);
    /// Claim for exclusive use
    pub const CLAIM: Permissions = Permissions(1 << 8);
    /// Request notification
    pub const NOTIFY: Permissions = Permissions(1 << 9);
}

impl Permissions {
    /// Create permissions with all bits set
    pub const fn all() -> Self {
        Self(u32::MAX)
    }

    /// Create empty permissions
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Create from raw bits
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Get the raw bits
    pub const fn bits(&self) -> u32 {
        self.0
    }

    /// Check if this contains the given permissions
    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Check if this has any of the given permissions
    pub fn contains_any(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    /// Check if this is empty
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Check if this has all permissions
    pub fn is_all(self) -> bool {
        self.0 == u32::MAX
    }

    /// Check if self is a subset of (or equal to) other permissions
    pub fn is_subset(self, other: Self) -> bool {
        (self.0 & other.0) == self.0
    }
}

impl From<u32> for Permissions {
    fn from(bits: u32) -> Self {
        Self(bits)
    }
}

impl From<Permissions> for u32 {
    fn from(perms: Permissions) -> Self {
        perms.0
    }
}

impl core::ops::BitOr for Permissions {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitAnd for Permissions {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl core::ops::BitXor for Permissions {
    type Output = Self;
    fn bitxor(self, rhs: Self) -> Self {
        Self(self.0 ^ rhs.0)
    }
}

impl core::ops::Not for Permissions {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0)
    }
}

impl core::ops::BitOrAssign for Permissions {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl core::ops::BitAndAssign for Permissions {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

/// Attenuation - restricting permissions (only reduces, never increases)
pub fn attenuate_permissions(parent: Permissions, child: Permissions) -> Permissions {
    child & parent
}

/// Check if child permissions are valid attenuation of parent
pub fn is_valid_attenuation(parent: Permissions, child: Permissions) -> bool {
    child.0 == (parent.0 & child.0)
}
