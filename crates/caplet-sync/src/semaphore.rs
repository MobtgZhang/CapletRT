//! Counting semaphore implementation
//!
//! A semaphore is a synchronization primitive that maintains a count. Tasks
//! can acquire (decrement) or release (increment) the count.
//!
//! ## Current Implementation
//!
//! The `acquire()` method uses a spin loop. This is appropriate for:
//! - Multi-core systems with short wait times
//! - Situations where the scheduler is not yet active
//! - Simple test scenarios
//!
//! ## Kernel Integration
//!
//! For RTOS production use, the semaphore should integrate with the kernel:
//! - Block tasks instead of spinning when count is 0
//! - Wake blocked tasks when count increments
//! - Support priority inheritance to prevent priority inversion
//!
//! ## Usage
//!
//! ```rust
//! use caplet_sync::Semaphore;
//!
//! let sem = Semaphore::new(1);  // Binary semaphore (like mutex)
//! assert!(sem.acquire());
//! // Critical section
//! // The count is exhausted, so a non-blocking acquire fails.
//! assert!(!sem.try_acquire());
//! sem.release();
//! // Released, so it can be taken again.
//! assert!(sem.try_acquire());
//! ```

use crate::spin::SpinLock;

/// A counting semaphore
pub struct Semaphore {
    inner: SpinLock<SemaphoreInner>,
}

#[derive(Debug)]
struct SemaphoreInner {
    count: isize,
    max: isize,
}

impl Semaphore {
    /// Create a new semaphore with given count
    pub const fn new(initial: isize) -> Self {
        Self {
            inner: SpinLock::new(SemaphoreInner {
                count: initial,
                max: isize::MAX,
            }),
        }
    }

    /// Create a bounded semaphore
    pub const fn bounded(initial: isize, max: isize) -> Self {
        Self {
            inner: SpinLock::new(SemaphoreInner {
                count: initial,
                max,
            }),
        }
    }

    /// Acquire the semaphore (decrements count)
    pub fn acquire(&self) -> bool {
        loop {
            let mut inner = self.inner.lock();
            if inner.count > 0 {
                inner.count -= 1;
                return true;
            }
            // Would need to wait
            drop(inner);
            core::hint::spin_loop();
        }
    }

    /// Try to acquire the semaphore without blocking
    pub fn try_acquire(&self) -> bool {
        let mut inner = self.inner.lock();
        if inner.count > 0 {
            inner.count -= 1;
            true
        } else {
            false
        }
    }

    /// Release the semaphore (increments count)
    pub fn release(&self) -> bool {
        let mut inner = self.inner.lock();
        if inner.count < inner.max {
            inner.count += 1;
            true
        } else {
            false
        }
    }

    /// Get current count
    pub fn count(&self) -> isize {
        self.inner.lock().count
    }
}

unsafe impl Send for Semaphore {}
unsafe impl Sync for Semaphore {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semaphore() {
        let sem = Semaphore::new(1);
        assert!(sem.try_acquire());
        assert!(!sem.try_acquire());
        sem.release();
        assert!(sem.try_acquire());
    }
}
