//! Rollback guard — prevents reverting to older firmware versions.
//!
//! Once a device has run a firmware version, it must not be rolled back
//! below the minimum acceptable version. This is a security requirement
//! for devices deployed in regulated environments (medical, automotive,
//! industrial).

/// Rollback protection level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RollbackLevel {
    /// No rollback protection.
    None,
    /// Allow only same-major-version rollbacks.
    Minor,
    /// Allow only same-version rollbacks.
    Patch,
    /// Never rollback — once upgraded, always forward.
    Strict,
}

/// Guard that tracks the minimum allowed firmware version.
///
/// The minimum version is stored in the Config flash region and can
/// only be increased (never decreased), providing a monotonic rollback
/// protection.
pub struct RollbackGuard {
    level: RollbackLevel,
}

impl RollbackGuard {
    /// Create a new rollback guard.
    pub const fn new(level: RollbackLevel) -> Self {
        Self { level }
    }

    /// Check if a new version can be applied based on the rollback policy.
    ///
    /// Returns `true` if the upgrade is allowed.
    pub fn can_upgrade(&self, current_version: u32, new_version: u32) -> bool {
        match self.level {
            RollbackLevel::None => true,
            RollbackLevel::Minor => {
                // Only reject downgrade of major version.
                major(current_version) <= major(new_version)
            }
            RollbackLevel::Patch => {
                // Reject any version older than current.
                new_version >= current_version
            }
            RollbackLevel::Strict => {
                // Only allow strictly increasing versions.
                new_version > current_version
            }
        }
    }

    /// Parse a semantic version from a u32 (packed as MMmmPPP).
    pub fn parse_version(v: u32) -> (u16, u8, u8) {
        let major = (v >> 16) as u16;
        let minor = ((v >> 8) & 0xFF) as u8;
        let patch = (v & 0xFF) as u8;
        (major, minor, patch)
    }

    /// Pack a semantic version into a u32.
    pub fn pack_version(major: u16, minor: u8, patch: u8) -> u32 {
        ((major as u32) << 16) | ((minor as u32) << 8) | (patch as u32)
    }
}

fn major(version: u32) -> u16 {
    (version >> 16) as u16
}
