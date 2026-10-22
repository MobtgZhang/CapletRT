//! Capability builder for fluent construction
//!
//! Provides a fluent API for constructing capabilities with custom
//! permissions, safety bits, and derivation options.
//!
//! [`CapBuilder::build`] creates a detached capability backed by an owned
//! resource.  [`CapBuilder::build_in`] additionally allocates a CSpace slot,
//! giving the capability kernel-visible revocation and generation semantics.

use crate::attenuation::DerivationOptions;
use crate::cap::Cap;
use crate::cspace::CSpace;
use crate::error::CapResult;
use crate::permissions::Permissions;
use crate::safety_bits::SafetyBits;

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

    /// Build a detached capability that owns `resource`.
    ///
    /// Detached capabilities support typed access and attenuation, but are
    /// not attached to a kernel CSpace until [`Self::build_in`] is used.
    pub fn build<T>(self, resource: T) -> Cap<T> {
        Cap::detached(resource, self.permissions, self.safety_bits)
    }

    /// Build a capability and insert it into `cspace`.
    ///
    /// # Errors
    ///
    /// Returns [`CapError::CSpaceFull`] or [`CapError::OutOfSlots`] when the
    /// CSpace cannot accept another capability.
    pub fn build_in<T: Send + 'static>(
        self,
        cspace: &mut CSpace,
        resource: T,
    ) -> CapResult<Cap<T>> {
        cspace.insert_cap(resource, self.permissions, self.safety_bits, None)
    }

    /// Build a child capability inside `cspace`, applying this builder's
    /// permissions and safety bits as an attenuation of `parent`.
    pub fn derive_in<T: Send + 'static>(
        self,
        cspace: &mut CSpace,
        parent: &Cap<T>,
    ) -> CapResult<Cap<T>> {
        cspace.derive_cap(parent, self.permissions, self.safety_bits)
    }
}

impl Default for CapBuilder {
    fn default() -> Self {
        Self::new()
    }
}
