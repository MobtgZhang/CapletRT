//! Capability wrapper types
//!
//! `Cap<T>` owns a shared, revocable resource cell.  A CSpace slot holds the
//! authorization metadata and a guard for the same cell, so revocation makes
//! every outstanding typed capability fail immediately.  Slots also carry a
//! generation, making stale handles harmless after slot reuse.

use crate::attenuation::DerivationOptions;
use crate::cspace::{next_detached_handle, WeakCSpace};
use crate::error::{CapError, CapResult};
use crate::handle::{Generation, Handle};
use crate::permissions::{perm, Permissions};
use crate::safety_bits::SafetyBits;
use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Type-erased revocation hook stored in a CSpace slot.
pub trait CapGuard: Send + Sync {
    /// Make every capability sharing this guard unusable.
    fn invalidate(&self);
    /// Whether the guarded resource is still accessible.
    fn is_valid(&self) -> bool;
    /// Record one additional CSpace authorization that references the
    /// guarded resource.
    ///
    /// Most guards are pure revocation tokens and do not need to track
    /// references.  Resource-backed guards override this so a granted slot
    /// can outlive the typed `Cap` from which it was copied.
    fn retain(&self) {}
    /// Drop one CSpace authorization previously recorded with [`Self::retain`].
    fn release(&self) {}
}

/// Shared storage for a capability's resource and its revocation state.
pub(crate) struct ResourceCell<T> {
    value: UnsafeCell<T>,
    valid: AtomicBool,
    /// Number of typed `Cap` handles.  A value of one permits `get_mut`.
    refs: AtomicU32,
    /// Number of CSpace slots that grant access to this cell.  Slot grants
    /// have no typed `Cap` in the destination CSpace, so they must keep the
    /// resource alive independently of `refs`.
    slot_refs: AtomicU32,
}

impl<T> ResourceCell<T> {
    pub(crate) fn new(value: T) -> Arc<Self> {
        Arc::new(Self {
            value: UnsafeCell::new(value),
            valid: AtomicBool::new(true),
            refs: AtomicU32::new(1),
            slot_refs: AtomicU32::new(0),
        })
    }

    pub(crate) fn retain(&self) {
        self.refs.fetch_add(1, Ordering::Relaxed);
    }

    fn release(&self) {
        loop {
            let refs = self.refs.load(Ordering::Acquire);
            if refs == 0 {
                return;
            }
            match self.refs.compare_exchange_weak(
                refs,
                refs - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    if refs == 1 {
                        self.invalidate_if_unreferenced();
                    }
                    return;
                }
                Err(_) => continue,
            }
        }
    }

    fn retain_slot(&self) {
        self.slot_refs.fetch_add(1, Ordering::Relaxed);
    }

    fn release_slot(&self) {
        loop {
            let refs = self.slot_refs.load(Ordering::Acquire);
            if refs == 0 {
                break;
            }
            if self
                .slot_refs
                .compare_exchange_weak(refs, refs - 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
        }
        self.invalidate_if_unreferenced();
    }

    fn invalidate_if_unreferenced(&self) {
        if self.refs.load(Ordering::Acquire) == 0 && self.slot_refs.load(Ordering::Acquire) == 0 {
            self.valid.store(false, Ordering::Release);
        }
    }

    fn read(&self) -> Option<&T> {
        if self.valid.load(Ordering::Acquire) {
            // SAFETY: mutable access is only granted when `refs == 1`, which
            // means no other Cap exists for this cell.  A unique `&mut self`
            // is required for `get_mut`, so immutable references cannot
            // overlap it.
            Some(unsafe { &*self.value.get() })
        } else {
            None
        }
    }

    // `Cap::get_mut` takes `&mut self`, and the reference-count check below
    // proves that this cell has exactly one outstanding capability.  The
    // only aliasing path would be a manually-forged `Cap`, which is not
    // constructible from the public API.
    #[allow(clippy::mut_from_ref)]
    fn write(&self) -> Option<&mut T> {
        if self.valid.load(Ordering::Acquire) && self.refs.load(Ordering::Acquire) == 1 {
            // SAFETY: see `read`; the reference count proves uniqueness.
            Some(unsafe { &mut *self.value.get() })
        } else {
            None
        }
    }
}

impl<T: Send + 'static> CapGuard for ResourceCell<T> {
    fn invalidate(&self) {
        self.valid.store(false, Ordering::Release);
    }

    fn is_valid(&self) -> bool {
        self.valid.load(Ordering::Acquire)
    }

    fn retain(&self) {
        self.retain_slot();
    }

    fn release(&self) {
        self.release_slot();
    }
}

// SAFETY: access to the UnsafeCell is gated by the validity and reference
// count checks above.  Sending the cell to another core requires `T: Send`.
unsafe impl<T: Send> Send for ResourceCell<T> {}
unsafe impl<T: Send> Sync for ResourceCell<T> {}

/// A capability is a type-safe reference to a resource.
///
/// A bound capability is linked to a CSpace slot.  A detached capability
/// (created through [`crate::CapBuilder::build`]) still provides typed access
/// but has no kernel table entry until inserted into a CSpace.
pub struct Cap<T> {
    handle: Handle<T>,
    permissions: Permissions,
    safety_bits: SafetyBits,
    resource: Option<Arc<ResourceCell<T>>>,
    cspace: Option<WeakCSpace>,
}

impl<T> Cap<T> {
    pub(crate) fn bound(
        handle: Handle<T>,
        permissions: Permissions,
        safety_bits: SafetyBits,
        resource: Arc<ResourceCell<T>>,
        cspace: WeakCSpace,
    ) -> Self {
        Self {
            handle,
            permissions,
            safety_bits,
            resource: Some(resource),
            cspace: Some(cspace),
        }
    }

    pub(crate) fn detached(resource: T, permissions: Permissions, safety_bits: SafetyBits) -> Self {
        let index = next_detached_handle();
        Self {
            handle: Handle::new(index, Generation::default()),
            permissions,
            safety_bits,
            resource: Some(ResourceCell::new(resource)),
            cspace: None,
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

    /// Whether this capability is still valid.
    pub fn is_valid(&self) -> bool {
        self.resource
            .as_ref()
            .map(|resource| resource.valid.load(Ordering::Acquire))
            .unwrap_or(false)
    }

    /// Whether this capability is bound to a CSpace slot.
    pub fn is_bound(&self) -> bool {
        self.cspace
            .as_ref()
            .and_then(|cspace| cspace.upgrade())
            .is_some()
    }

    /// Check if this capability has the given permissions
    pub fn has_permission(&self, perm: Permissions) -> bool {
        self.permissions.contains(perm)
    }

    /// Get an immutable reference to the resource.
    pub fn get(&self) -> CapResult<&T> {
        if !self.permissions.contains(perm::READ) {
            return Err(CapError::Permission);
        }
        self.resource
            .as_ref()
            .and_then(|resource| resource.read())
            .ok_or(CapError::Revoked)
    }

    /// Get a mutable reference to the resource.
    ///
    /// This succeeds only when this is the sole outstanding capability for the
    /// resource.  Aliased capabilities deliberately fail with
    /// [`CapError::Permission`] rather than creating an aliasing `&mut T`.
    pub fn get_mut(&mut self) -> CapResult<&mut T> {
        if !self.permissions.contains(perm::WRITE) {
            return Err(CapError::Permission);
        }
        self.resource
            .as_ref()
            .and_then(|resource| resource.write())
            .ok_or(CapError::Permission)
    }

    pub(crate) fn resource(&self) -> Option<Arc<ResourceCell<T>>> {
        self.resource.clone()
    }
}

impl<T> Clone for Cap<T> {
    fn clone(&self) -> Self {
        if let Some(resource) = &self.resource {
            resource.retain();
        }
        let mut handle = self.handle;
        if let Some(cspace) = self.cspace.as_ref().and_then(|cspace| cspace.upgrade()) {
            if let Some(generation) = cspace.lock().retain_slot(handle.index()) {
                handle = Handle::new(handle.index(), generation);
            }
        }
        Self {
            handle,
            permissions: self.permissions,
            safety_bits: self.safety_bits,
            resource: self.resource.clone(),
            cspace: self.cspace.clone(),
        }
    }
}

impl<T> Drop for Cap<T> {
    fn drop(&mut self) {
        if let Some(cspace) = self.cspace.as_ref().and_then(|cspace| cspace.upgrade()) {
            cspace
                .lock()
                .release_slot(self.handle.index(), self.handle.generation());
        }
        if let Some(resource) = &self.resource {
            resource.release();
        }
    }
}

/// Derive a new capability with restricted permissions.
pub fn derive<T: Send + 'static>(cap: &Cap<T>, options: DerivationOptions) -> CapResult<Cap<T>> {
    if !cap.permissions.contains(perm::DERIVE) {
        return Err(CapError::Permission);
    }
    let new_permissions = match options.attenuation() {
        Some(attenuation) => attenuation.apply(cap.permissions)?,
        None => cap.permissions,
    };
    let new_safety = cap.safety_bits;
    if !crate::safety_policy::SafetyPolicy::validate_derivation(
        new_safety,
        new_permissions,
        cap.safety_bits,
        cap.permissions,
    )
    .is_valid()
    {
        return Err(CapError::InvalidDerivation);
    }

    let cspace = cap
        .cspace
        .as_ref()
        .and_then(|cspace| cspace.upgrade())
        .ok_or(CapError::Invalid)?;
    let resource = cap.resource.as_ref().ok_or(CapError::Invalid)?.clone();
    let guard: Arc<dyn CapGuard> = resource.clone();
    let handle = cspace.lock().allocate_slot(
        new_permissions,
        new_safety,
        Some(cap.handle.index()),
        Some(guard),
    )?;
    let handle = Handle::<T>::new(handle.index(), handle.generation());
    resource.retain();
    Ok(Cap::bound(
        handle,
        new_permissions,
        new_safety,
        resource,
        cap.cspace.clone().ok_or(CapError::Invalid)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attenuation::Attenuation;
    use crate::cspace::CSpace;

    fn all_permissions() -> Permissions {
        perm::READ | perm::WRITE | perm::DERIVE
    }

    #[test]
    fn detached_cap_provides_real_access() {
        let mut cap = crate::CapBuilder::new().build(41u32);
        assert_eq!(*cap.get().unwrap(), 41);
        *cap.get_mut().unwrap() = 42;
        assert_eq!(*cap.get().unwrap(), 42);
    }

    #[test]
    fn read_requires_read_permission() {
        let cap = crate::CapBuilder::new()
            .with_permissions(perm::WRITE)
            .build(7u32);
        assert_eq!(cap.get(), Err(CapError::Permission));
    }

    #[test]
    fn write_requires_write_permission() {
        let mut cap = crate::CapBuilder::new()
            .with_permissions(perm::READ)
            .build(7u32);
        assert_eq!(cap.get_mut(), Err(CapError::Permission));
    }

    #[test]
    fn bound_cap_accesses_its_resource() {
        let mut cspace = CSpace::new();
        let cap = cspace
            .insert_cap(9u32, all_permissions(), SafetyBits::default(), None)
            .unwrap();
        assert!(cap.is_bound());
        assert_eq!(*cap.get().unwrap(), 9);
        assert_eq!(cspace.active_count(), 1);
    }

    #[test]
    fn revoke_invalidates_bound_cap_immediately() {
        let mut cspace = CSpace::new();
        let cap = cspace
            .insert_cap(11u32, all_permissions(), SafetyBits::default(), None)
            .unwrap();
        cspace.revoke(cap.handle().index()).unwrap();
        assert_eq!(cap.get(), Err(CapError::Revoked));
        assert!(!cap.is_valid());
    }

    #[test]
    fn cloned_cap_cannot_alias_mutable_access() {
        let mut cspace = CSpace::new();
        let cap = cspace
            .insert_cap(5u32, all_permissions(), SafetyBits::default(), None)
            .unwrap();
        let clone = cap.clone();
        let mut cap = cap;
        assert_eq!(cap.get_mut(), Err(CapError::Permission));
        assert_eq!(*clone.get().unwrap(), 5);
        drop(clone);
        *cap.get_mut().unwrap() = 6;
        assert_eq!(*cap.get().unwrap(), 6);
    }

    #[test]
    fn dropping_last_cap_reclaims_slot() {
        let mut cspace = CSpace::new();
        let cap = cspace
            .insert_cap(1u32, all_permissions(), SafetyBits::default(), None)
            .unwrap();
        assert_eq!(cspace.active_count(), 1);
        drop(cap);
        assert_eq!(cspace.active_count(), 0);
    }

    #[test]
    fn dropping_one_clone_keeps_slot_alive() {
        let mut cspace = CSpace::new();
        let cap = cspace
            .insert_cap(1u32, all_permissions(), SafetyBits::default(), None)
            .unwrap();
        let clone = cap.clone();
        drop(cap);
        assert_eq!(cspace.active_count(), 1);
        assert_eq!(*clone.get().unwrap(), 1);
    }

    #[test]
    fn derive_attenuates_permissions() {
        let mut cspace = CSpace::new();
        let parent = cspace
            .insert_cap(77u32, all_permissions(), SafetyBits::default(), None)
            .unwrap();
        let options = DerivationOptions::new().attenuate(Attenuation::new(perm::READ));
        let child = derive(&parent, options).unwrap();
        assert_eq!(child.permissions(), perm::READ);
        assert_eq!(*child.get().unwrap(), 77);
        assert_eq!(cspace.active_count(), 2);
    }

    #[test]
    fn derive_requires_derive_permission() {
        let mut cspace = CSpace::new();
        let parent = cspace
            .insert_cap(1u32, perm::READ, SafetyBits::default(), None)
            .unwrap();
        assert!(matches!(
            derive(&parent, DerivationOptions::new()),
            Err(CapError::Permission)
        ));
    }

    #[test]
    fn derive_rejects_permission_amplification() {
        let mut cspace = CSpace::new();
        let parent = cspace
            .insert_cap(1u32, perm::READ | perm::DERIVE, SafetyBits::default(), None)
            .unwrap();
        let options = DerivationOptions::new()
            .attenuate(Attenuation::new(perm::READ | perm::WRITE | perm::DERIVE));
        assert!(matches!(
            derive(&parent, options),
            Err(CapError::InvalidDerivation)
        ));
    }

    #[test]
    fn detached_cap_derive_is_rejected() {
        let parent = crate::CapBuilder::new().build(1u32);
        assert!(matches!(
            derive(&parent, DerivationOptions::new()),
            Err(CapError::Invalid)
        ));
    }
}
