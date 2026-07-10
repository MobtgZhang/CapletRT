//! Event flags for synchronization
//!
//! Events provide a way for tasks to wait for specific conditions. A task can
//! wait for one or more flags to be set.
//!
//! ## Current Implementation
//!
//! Waiting methods use spin loops. For production RTOS use, these should
//! block the calling task and wake it when the condition is met.
//!
//! ## Usage
//!
//! ```rust
//! let event = Event::new();
//! // Task A: wait for flag 1 or 2
//! event.wait_any(1 | 2);
//!
//! // Task B: set flags
//! event.set(1);
//! ```

use crate::spin::SpinLock;

/// Event flags for task synchronization
pub struct Event {
    inner: SpinLock<EventInner>,
}

#[derive(Debug, Default)]
struct EventInner {
    flags: u32,
    waiters: u32,
}

impl Event {
    /// Create a new event with no flags set
    pub const fn new() -> Self {
        Self {
            inner: SpinLock::new(EventInner {
                flags: 0,
                waiters: 0,
            }),
        }
    }

    /// Create a new event with initial flags set
    pub const fn with_flags(initial: u32) -> Self {
        Self {
            inner: SpinLock::new(EventInner {
                flags: initial,
                waiters: 0,
            }),
        }
    }

    /// Set flags (OR operation)
    pub fn set(&self, flags: u32) {
        self.inner.lock().flags |= flags;
    }

    /// Clear flags
    pub fn clear(&self, flags: u32) {
        self.inner.lock().flags &= !flags;
    }

    /// Get current flags
    pub fn flags(&self) -> u32 {
        self.inner.lock().flags
    }

    /// Wait for all specified flags to be set
    pub fn wait_all(&self, flags: u32) {
        loop {
            {
                let mut inner = self.inner.lock();
                if (inner.flags & flags) == flags {
                    inner.flags &= !flags; // Clear the matched flags
                    return;
                }
                inner.waiters += 1;
            }
            core::hint::spin_loop();
        }
    }

    /// Wait for any of the specified flags to be set
    pub fn wait_any(&self, flags: u32) -> u32 {
        loop {
            {
                let mut inner = self.inner.lock();
                let matched = inner.flags & flags;
                if matched != 0 {
                    inner.flags &= !matched;
                    return matched;
                }
                inner.waiters += 1;
            }
            core::hint::spin_loop();
        }
    }

    /// Try to wait without blocking
    pub fn try_wait(&self, flags: u32) -> Option<u32> {
        let mut inner = self.inner.lock();
        let matched = inner.flags & flags;
        if matched != 0 {
            inner.flags &= !matched;
            Some(matched)
        } else {
            None
        }
    }

    /// Check if any of the specified flags are set
    pub fn check(&self, flags: u32) -> bool {
        (self.inner.lock().flags & flags) != 0
    }

    /// Get the number of waiters
    pub fn waiter_count(&self) -> u32 {
        self.inner.lock().waiters
    }
}

unsafe impl Send for Event {}
unsafe impl Sync for Event {}