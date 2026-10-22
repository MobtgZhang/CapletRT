//! Capability derivation

use crate::attenuation::DerivationOptions;
use crate::error::{CapError, CapResult};
use crate::handle::{Generation, Handle};
use crate::parent_link::ParentLink;
use crate::permissions::Permissions;
use crate::safety_bits::SafetyBits;
use crate::safety_policy::SafetyPolicy;

/// Result of a successful derivation
#[derive(Debug, Clone, Copy)]
pub struct DerivedCapability<T> {
    /// The handle
    pub handle: Handle<T>,
    /// The permissions
    pub permissions: Permissions,
    /// The safety bits
    pub safety_bits: SafetyBits,
    /// The parent handle
    pub parent: Handle<T>,
}

impl<T> DerivedCapability<T> {
    /// Create a new derived capability
    pub fn new(
        handle: Handle<T>,
        parent: Handle<T>,
        permissions: Permissions,
        safety_bits: SafetyBits,
    ) -> Self {
        Self {
            handle,
            permissions,
            safety_bits,
            parent,
        }
    }
}

/// Derive a new capability from a parent
pub fn cap_derive<T>(
    parent_handle: Handle<T>,
    parent_permissions: Permissions,
    parent_safety: SafetyBits,
    _options: DerivationOptions,
) -> CapResult<DerivedCapability<T>> {
    let new_handle = Handle::new(parent_handle.index(), Generation::default());
    Ok(DerivedCapability::new(
        new_handle,
        parent_handle,
        parent_permissions,
        parent_safety,
    ))
}

/// Validate that a derivation is allowed
pub fn validate_derivation(
    child_safety: SafetyBits,
    child_perms: Permissions,
    parent_safety: SafetyBits,
    parent_perms: Permissions,
) -> bool {
    SafetyPolicy::validate_derivation(child_safety, child_perms, parent_safety, parent_perms)
        .is_valid()
}

/// Create a new derivation with custom permissions
pub fn derive_with_perms<T>(
    parent_handle: Handle<T>,
    parent_permissions: Permissions,
    parent_safety: SafetyBits,
    new_permissions: Permissions,
) -> CapResult<DerivedCapability<T>> {
    if !SafetyPolicy::validate_permissions(new_permissions, parent_permissions) {
        return Err(CapError::InvalidDerivation);
    }

    let attenuated = SafetyPolicy::restrictive_permissions(new_permissions, parent_permissions);
    let attenuated_safety = SafetyPolicy::restrictive_safety(parent_safety, parent_safety);

    Ok(DerivedCapability::new(
        Handle::new(parent_handle.index(), Generation::default()),
        parent_handle,
        attenuated,
        attenuated_safety,
    ))
}

/// Get the parent link for a capability
pub fn make_parent_link(parent_index: u32) -> ParentLink {
    ParentLink::child(parent_index)
}
