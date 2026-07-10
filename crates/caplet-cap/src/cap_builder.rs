//! Capability builder for fluent construction
//!
//! Provides a fluent API for constructing capabilities with custom
//! permissions, safety bits, and derivation options.
//!
//! Note: The `build()` method requires kernel integration. In standalone
//! library mode, it returns an error. When integrated with the kernel's
//! CSpace management, this can be properly implemented.

use crate::cap::Cap;
use crate::error::{CapError, CapResult};
use crate::handle::Handle;
use crate::permissions::Permissions;
use crate::safety_bits::SafetyBits;
use crate::attenuation::DerivationOptions;

/// Builder for creating capabilities with a fluent API
pub struct CapBuilder {
    permissions: Permissions,
    safety_bits: SafetyBits,
    options: DerivationOptions,
}

impl CapBuilder {
    /// Create a new capability builder
    pub fn new() -> Self {
        Self {
            permissions: Permissions::all(),
            safety_bits: SafetyBits::default(),
            options: DerivationOptions::default(),
        }
    }

    /// Set the permissions for the capability
    pub fn with_permissions(mut self, perms: Permissions) -> Self {
        self.permissions = perms;
        self
    }

    /// Add a permission to the capability
    pub fn add_permission(mut self, perm: Permissions) -> Self {
        self.permissions |= perm;
        self
    }

    /// Remove a permission from the capability
    pub fn remove_permission(mut self, perm: Permissions) -> Self {
        self.permissions &= !perm;
        self
    }

    /// Set the safety bits
    pub fn with_safety_bits(mut self, bits: SafetyBits) -> Self {
        self.safety_bits = bits;
        self
    }

    /// Enable audit for this capability
    pub fn with_audit(mut self) -> Self {
        self.safety_bits.set_audit(true);
        self
    }

    /// Enable persistent for this capability
    pub fn with_persistent(mut self) -> Self {
        self.safety_bits.set_persistent(true);
        self
    }

    /// Disable transfer for this capability
    pub fn non_transferable(mut self) -> Self {
        self.safety_bits.set_transferable(false);
        self
    }

    /// Set the time-to-live for this capability (in milliseconds)
    pub fn with_ttl(mut self, ms: u64) -> Self {
        self.options = self.options.with_ttl(ms);
        self
    }

    /// Set the maximum use count for this capability
    pub fn with_max_uses(mut self, count: u32) -> Self {
        self.options = self.options.with_max_uses(count);
        self
    }

    /// Build the capability.
    ///
    /// In standalone library mode, this returns an error because building
    /// a capability requires access to the kernel's CSpace for slot allocation.
    ///
    /// When integrated with the kernel, this should:
    /// 1. Request a new slot from the CSpace
    /// 2. Initialize the capability metadata
    /// 3. Return the capability wrapping the allocated handle
    ///
    /// # Errors
    ///
    /// Returns `CapError::OutOfSlots` if the CSpace is full.
    /// Returns `CapError::Invalid` if the builder configuration is invalid.
    pub fn build<T>(self) -> CapResult<Cap<T>> {
        // In standalone mode, we cannot allocate slots without kernel context.
        // The kernel integration provides a CSpace backend that can allocate slots.
        //
        // To use CapBuilder with the kernel:
        // 1. Have the kernel provide a `CSpace::allocate()` method
        // 2. Pass the CSpace reference through a thread-local or global
        // 3. Replace this stub with actual slot allocation
        Err(CapError::Invalid)
    }
}

impl Default for CapBuilder {
    fn default() -> Self {
        Self::new()
    }
}