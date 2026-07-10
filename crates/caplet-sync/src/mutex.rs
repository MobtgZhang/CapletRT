//! Mutex implementation
//!
//! This module provides a basic mutex implementation using spin locks.
//!
//! ## Performance Considerations
//!
//! In this implementation, the `lock()` method uses a spin loop. This is
//! appropriate for short critical sections on multi-core systems or when
//! the lock is rarely contended.
//!
//! ## Kernel Integration
//!
//! For production use with the CapletRT kernel, a priority-inheriting mutex
//! should be used to prevent priority inversion. This would integrate with
//! the kernel scheduler to:
//! - Block the task instead of spinning when the lock is held
//! - Inherit the priority of higher-priority waiters
//! - Wake blocked tasks when the lock is released
//!
//! The current spin-based implementation is suitable for:
//! - Boot-time initialization (before scheduler is active)
//! - Protecting data structures that are always accessed from one task
//! - Simple test scenarios

use crate::spin::SpinLock;
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};

/// A mutex (priority-aware)
pub struct Mutex<T> {
    /// Inner data protected by a spin lock
    inner: SpinLock<MutexInner<T>>,
}

/// Inner state of a mutex
struct MutexInner<T> {
    /// Whether the mutex is locked
    locked: bool,
    /// The protected data
    data: UnsafeCell<T>,
}

impl<T> Mutex<T> {
    /// Create a new mutex
    pub const fn new(data: T) -> Self {
        Self {
            inner: SpinLock::new(MutexInner {
                locked: false,
                data: UnsafeCell::new(data),
            }),
        }
    }

    /// Try to lock the mutex without blocking
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        let mut inner = self.inner.lock();
        if inner.locked {
            None
        } else {
            inner.locked = true;
            Some(MutexGuard { mutex: self })
        }
    }

    /// Lock the mutex.
    ///
    /// This method uses a spin loop until the lock is acquired. For short
    /// critical sections, this is efficient. For longer waits or single-task
    /// scenarios, consider using `try_lock()` in a loop with task yielding.
    ///
    /// # Performance
    ///
    /// On multi-core systems, this may cause busy-waiting. For production
    /// RTOS use, integrate with the kernel scheduler to block the task.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        loop {
            {
                let mut inner = self.inner.lock();
                if !inner.locked {
                    inner.locked = true;
                    return MutexGuard { mutex: self };
                }
            }
            // Yield to scheduler
            core::hint::spin_loop();
        }
    }

    /// Consume the mutex and return the inner data
    pub fn into_inner(self) -> T {
        self.inner.into_inner().data.into_inner()
    }
}

unsafe impl<T: Send> Send for Mutex<T> {}
unsafe impl<T: Send> Sync for Mutex<T> {}

/// RAII guard for Mutex
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<'a, T> MutexGuard<'a, T> {
    /// Access the underlying data
    pub fn data(&self) -> &T {
        // SAFETY: We hold the lock
        unsafe { &*self.mutex.inner.lock().data.get() }
    }

    /// Access the underlying data mutably
    pub fn data_mut(&mut self) -> &mut T {
        // SAFETY: We hold the lock exclusively
        unsafe { &mut *self.mutex.inner.lock().data.get() }
    }
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data()
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data_mut()
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        self.mutex.inner.lock().locked = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutex() {
        let m = Mutex::new(42);
        {
            let guard = m.lock();
            assert_eq!(*guard, 42);
        }
        {
            let guard = m.lock();
            assert_eq!(*guard, 42);
        }
    }
}