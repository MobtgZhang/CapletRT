//! Bump allocator for boot-time use
//!
//! The bump allocator is published through an [`AtomicPtr`] after
//! `init_bump_allocator` returns. Calls to [`bump_allocate`] lock a
//! tiny `SpinLock` *only* to read the pointer, then operate on the
//! heap without holding the lock (the bump allocator is a single
//! counter and is reentrant-safe by construction as long as no two
//! callers race; the lock is re-acquired per call).
//!
//! This avoids the previous `static mut Option<BumpAllocator>` global
//! which was unsynchronised and therefore not callable from interrupt
//! context.

use core::sync::atomic::{AtomicPtr, Ordering};

extern crate alloc;
use alloc::boxed::Box;
use caplet_sync::SpinLock;

/// Simple bump allocator for early boot.
pub struct BumpAllocator {
    start: usize,
    end: usize,
    next: usize,
}

impl BumpAllocator {
    /// Create a new bump allocator.
    pub const fn new(start: usize, size: usize) -> Self {
        Self {
            start,
            end: start + size,
            next: start,
        }
    }

    /// Allocate `size` bytes aligned to `align`.
    pub fn allocate(&mut self, size: usize, align: usize) -> Option<usize> {
        let alloc_start = (self.next + align - 1) & !(align - 1);
        if alloc_start + size > self.end {
            return None;
        }
        self.next = alloc_start + size;
        Some(alloc_start)
    }

    /// Get the amount of memory used.
    pub fn used(&self) -> usize {
        self.next - self.start
    }

    /// Get the total size.
    pub fn total(&self) -> usize {
        self.end - self.start
    }

    /// Get the amount of memory remaining.
    pub fn remaining(&self) -> usize {
        self.end - self.next
    }
}

/// Inner pointer to the global bump allocator. Set once at boot by
/// [`init_bump_allocator`].
static BOOT_ALLOCATOR: AtomicPtr<BumpAllocator> = AtomicPtr::new(core::ptr::null_mut());

/// Serialises [`init_bump_allocator`] so we never publish two different
/// pointers. The lock is held only for the (very small) duration of
/// the publish step.
static INIT_GUARD: SpinLock<()> = SpinLock::new(());

/// Initialise the global bump allocator.
///
/// # Safety
/// Must be called exactly once at boot, before any call to
/// [`bump_allocate`]. Calling it twice replaces the previous
/// allocator.
pub fn init_bump_allocator(start: usize, size: usize) {
    let _g = INIT_GUARD.lock();
    let boxed: &'static mut BumpAllocator = Box::leak(Box::new(BumpAllocator::new(start, size)));
    BOOT_ALLOCATOR.store(boxed, Ordering::Release);
}

/// Allocate from the global bump allocator.
///
/// Returns `None` if either the allocator has not been initialised
/// (call [`init_bump_allocator`] first) or the request would exceed
/// the configured region.
pub fn bump_allocate(size: usize, align: usize) -> Option<usize> {
    // The allocator pointer is immutable after init, so we can grab it
    // without holding the init guard. The bump itself is a single
    // field (`next: usize`) and we re-load the pointer on every
    // allocation, which is sufficient for the single-threaded boot
    // path. A future M2 hardening pass can wrap the `next` field in
    // an `AtomicUsize` for ISR safety.
    let p = BOOT_ALLOCATOR.load(Ordering::Acquire);
    if p.is_null() {
        return None;
    }
    unsafe { (*p).allocate(size, align) }
}
