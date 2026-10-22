//! Kernel-side bookkeeping for security peripheral capabilities.
//!
//! The capability table records *that* a slot exists and which permissions
//! it carries.  Peripheral-specific metadata (for example the flash byte
//! range granted by a `FlashToken`) lives here, keyed by the slot index in
//! the caller's CSpace.  Registration is performed by the kernel when it
//! installs a capability; user code cannot add grants.

use crate::kernel::with_current_cspace;
use ::caplet_cap::permissions::perm;
use ::caplet_cap::{CSpace, Permissions, SafetyBits};
use alloc::vec::Vec;
use caplet_sync::SpinLock;

/// Name under which the kernel publishes the default crypto capability.
pub const CRYPTO_CAP_NAME: &str = "crypto:0";
/// Name under which the kernel publishes the default flash capability.
pub const FLASH_CAP_NAME: &str = "flash:0";

/// Metadata attached to a flash capability slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashGrant {
    /// First byte address granted to the holder (inclusive).
    pub start: u32,
    /// Last byte address granted to the holder (inclusive).
    pub end: u32,
    /// Whether erase/program operations are permitted.
    pub writable: bool,
    /// Erase sector size for the grant.
    pub sector_size: u32,
}

impl FlashGrant {
    /// `true` when `[start, end]` is fully inside the grant.
    pub fn contains(&self, start: u32, end: u32) -> bool {
        start <= end && start >= self.start && end <= self.end
    }
}

#[derive(Clone, Copy)]
struct FlashEntry {
    slot: u32,
    grant: FlashGrant,
}

static FLASH_GRANTS: SpinLock<Vec<FlashEntry>> = SpinLock::new(Vec::new());

/// Error returned when installing a security capability fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantError {
    /// The CSpace rejected the new slot.
    CapSpace,
}

fn install_named_cap(name: &str, permissions: Permissions) -> Result<u32, GrantError> {
    with_current_cspace(|cspace: &mut CSpace| {
        let handle = cspace
            .allocate(permissions, SafetyBits::default(), None)
            .map_err(|_| GrantError::CapSpace)?;
        cspace
            .bind_name(handle.index(), name)
            .map_err(|_| GrantError::CapSpace)?;
        Ok(handle.index())
    })
}

/// Install the default crypto capability in the caller's CSpace.
///
/// This is a kernel/bootstrap helper.  It is public so board bring-up code
/// and tests can model the manifest grant the loader would normally perform.
pub fn install_crypto_capability() -> Result<u32, GrantError> {
    let index = install_named_cap(CRYPTO_CAP_NAME, perm::READ | perm::WRITE)?;
    Ok(index)
}

/// Install a named flash capability and record its byte range.
pub fn install_flash_capability(
    start: u32,
    end: u32,
    writable: bool,
    sector_size: u32,
) -> Result<u32, GrantError> {
    if end < start || sector_size == 0 {
        return Err(GrantError::CapSpace);
    }
    let permissions = if writable {
        perm::READ | perm::WRITE
    } else {
        perm::READ
    };
    let slot = install_named_cap(FLASH_CAP_NAME, permissions)?;
    let mut grants = FLASH_GRANTS.lock();
    grants.retain(|entry| entry.slot != slot);
    grants.push(FlashEntry {
        slot,
        grant: FlashGrant {
            start,
            end,
            writable,
            sector_size,
        },
    });
    Ok(slot)
}

/// Remove all installed security capabilities and grants.
///
/// This is intended for test teardown and kernel re-initialisation.
pub fn clear_security_grants() {
    FLASH_GRANTS.lock().clear();
    with_current_cspace(|cspace| {
        cspace.revoke_all_fast();
    });
}

/// Check whether the current CSpace holds a live crypto capability with
/// read permission.
pub fn crypto_allowed() -> bool {
    with_current_cspace(|cspace| {
        let Some(index) = cspace.find_by_name(CRYPTO_CAP_NAME) else {
            return false;
        };
        cspace
            .slot_perms_safety(index)
            .map(|(perms, _)| perms.contains(perm::READ))
            .unwrap_or(false)
    })
}

/// Look up the flash grant belonging to the current CSpace's flash
/// capability, together with the capability permissions.
pub fn current_flash_grant() -> Option<(FlashGrant, Permissions)> {
    with_current_cspace(|cspace| {
        let index = cspace.find_by_name(FLASH_CAP_NAME)?;
        let (perms, _) = cspace.slot_perms_safety(index)?;
        if !perms.contains(perm::READ) {
            return None;
        }
        let grants = FLASH_GRANTS.lock();
        let grant = grants
            .iter()
            .rev()
            .find(|entry| entry.slot == index)
            .map(|entry| entry.grant)?;
        Some((grant, perms))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_contains_is_inclusive() {
        let grant = FlashGrant {
            start: 0x1000_0000,
            end: 0x1000_0fff,
            writable: true,
            sector_size: 4096,
        };
        assert!(grant.contains(0x1000_0000, 0x1000_0fff));
        assert!(!grant.contains(0x1000_0000, 0x1000_1000));
        assert!(!grant.contains(0x0fff_ffff, 0x1000_0000));
    }
}
