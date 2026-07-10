//! Capability wrapper types
//!
//! The capability system is designed to work in two modes:
//! 1. **Standalone library mode**: `Cap<T>` is a lightweight reference wrapper
//! 2. **Kernel integration mode**: Resources are managed by CSpace, and this
//!    crate provides the type-safe wrapper around handle indices.
//!
//! In kernel mode, `Cap::get()` and `Cap::get_mut()` require access to the
//! global CSpace, which is provided by the kernel. The standalone library
//! cannot implement these without a kernel context, so they return errors.

use crate::error::{CapError, CapResult};
use crate::handle::Handle;
use crate::permissions::Permissions;
use crate::safety_bits::SafetyBits;
use crate::attenuation::DerivationOptions;

/// A capability is a type-safe reference to a resource.
///
/// In the standalone library, a `Cap<T>` wraps a `Handle<T>` (which is just
/// an index into a CSpace table) along with permission bits. The actual
/// resource access requires integration with the kernel's CSpace management.
///
/// For resources managed by the kernel:
/// - `Cap<Uart>` provides exclusive access to a UART peripheral
/// - `Cap<Gpio>` provides access to GPIO pins
/// - `Cap<MemoryRegion>` provides access to a memory region
#[derive(Debug)]
pub struct Cap<T> {
    handle: Handle<T>,
    permissions: Permissions,
    safety_bits: SafetyBits,
}

impl<T> Cap<T> {
    /// Create a new capability (internal use only)
    pub(crate) fn new(handle: Handle<T>, permissions: Permissions, safety_bits: SafetyBits) -> Self {
        Self {
            handle,
            permissions,
            safety_bits,
        }
    }

    /// Get the underlying handle
    pub fn handle(&self) -> Handle<T> {
        self.handle
    }

    /// Get the permissions for this capability
    pub fn permissions(&self) -> Permissions {
        self.permissions
    }

    /// Get the safety bits for this capability
    pub fn safety_bits(&self) -> SafetyBits {
        self.safety_bits
    }

    /// Check if this capability has the given permissions
    pub fn has_permission(&self, perm: Permissions) -> bool {
        self.permissions.contains(perm)
    }

    /// Get an immutable reference to the resource.
    ///
    /// # Error
    ///
    /// Returns `CapError::Permission` if the capability lacks READ permission.
    /// Returns `CapError::Invalid` if this is used outside of kernel context.
    ///
    /// # Note
    ///
    /// In standalone library mode (without kernel), this always returns
    /// `CapError::Invalid`. When integrated with the kernel, the CSpace
    /// provides the actual resource access.
    pub fn get(&self) -> CapResult<&T> {
        if !self.permissions.contains(crate::permissions::perm::READ) {
            return Err(CapError::Permission);
        }
        // In kernel mode, this would look up the resource in CSpace:
        // let resource = cspace.resolve(&self.handle)?;
        // Ok(resource)
        Err(CapError::Invalid)
    }

    /// Get a mutable reference to the resource.
    ///
    /// # Error
    ///
    /// Returns `CapError::Permission` if the capability lacks WRITE permission.
    /// Returns `CapError::Invalid` if this is used outside of kernel context.
    ///
    /// # Note
    ///
    /// In standalone library mode (without kernel), this always returns
    /// `CapError::Invalid`. When integrated with the kernel, the CSpace
    /// provides the actual resource access.
    pub fn get_mut(&mut self) -> CapResult<&mut T> {
        if !self.permissions.contains(crate::permissions::perm::WRITE) {
            return Err(CapError::Permission);
        }
        // In kernel mode, this would look up and lock the resource in CSpace:
        // let resource = cspace.resolve_mut(&mut self.handle)?;
        // Ok(resource)
        Err(CapError::Invalid)
    }
}

impl<T> Clone for Cap<T> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle,
            permissions: self.permissions,
            safety_bits: self.safety_bits,
        }
    }
}

impl<T> Drop for Cap<T> {
    fn drop(&mut self) {
        // When integrated with kernel, this would:
        // - Release the slot in CSpace
        // - Decrement reference count
        // - Potentially trigger revocation of child capabilities
    }
}

/// Derive a new capability with restricted permissions.
///
/// In standalone mode, this requires DERIVE permission and creates a
/// new capability with attenuated permissions.
///
/// In kernel mode, the CSpace handles the actual derivation and slot
/// allocation.
pub fn derive<T>(
    cap: &Cap<T>,
    _options: DerivationOptions,
) -> CapResult<Cap<T>> {
    if !cap.permissions.contains(crate::permissions::perm::READ) {
        return Err(CapError::Permission);
    }
    // In kernel mode, this would:
    // - Check derivation rights
    // - Allocate new slot in CSpace
    // - Copy handle with attenuated permissions
    // - Establish parent-child relationship
    Err(CapError::Invalid)
}