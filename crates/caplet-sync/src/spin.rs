//! Spin lock implementation

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use core::ops::{Deref, DerefMut};

/// A simple spin lock
///
/// Spin locks are used for short critical sections where the contention
/// is expected to be low. In an RTOS, they're typically used for very
/// brief operations like updating a small data structure.
pub struct SpinLock<T: ?Sized> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

impl<T> SpinLock<T> {
    /// Create a new spin lock
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// Try to acquire the lock without blocking
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        if self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(SpinLockGuard { lock: self })
        } else {
            None
        }
    }

    /// Lock the spin lock (blocks until acquired)
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        loop {
            if let Some(guard) = self.try_lock() {
                return guard;
            }
            // Busy wait hint
            core::hint::spin_loop();
        }
    }

    /// Check if the lock is currently held
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }

    /// Consume the spin lock and return the inner data
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

unsafe impl<T: ?Sized + Send> Send for SpinLock<T> {}
unsafe impl<T: ?Sized + Send> Sync for SpinLock<T> {}

/// RAII guard for SpinLock
pub struct SpinLockGuard<'a, T: ?Sized> {
    lock: &'a SpinLock<T>,
}

impl<'a, T: ?Sized> SpinLockGuard<'a, T> {
    /// Access the underlying data
    pub fn data(&self) -> &T {
        // SAFETY: We hold the lock
        unsafe { &*self.lock.data.get() }
    }

    /// Access the underlying data mutably
    pub fn data_mut(&mut self) -> &mut T {
        // SAFETY: We hold the lock exclusively
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T: ?Sized> Deref for SpinLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data()
    }
}

impl<'a, T: ?Sized> DerefMut for SpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data_mut()
    }
}

impl<'a, T: ?Sized> Drop for SpinLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spin_lock() {
        let lock = SpinLock::new(42);
        {
            let guard = lock.lock();
            assert_eq!(*guard, 42);
        }
        assert!(!lock.is_locked());
    }

    #[test]
    fn test_try_lock() {
        let lock = SpinLock::new(0);
        let guard1 = lock.try_lock().unwrap();
        assert!(lock.try_lock().is_none());
        drop(guard1);
        assert!(lock.try_lock().is_some());
    }
}