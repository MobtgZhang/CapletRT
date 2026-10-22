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
//! use caplet_sync::Event;
//!
//! let event = Event::new();
//! // Task B: set flags for 1 and 2.
//! event.set(1 | 2);
//! // Task A: wait for flag 1 or 2; matched flags are consumed.
//! let matched = event.wait_any(1 | 2);
//! assert_eq!(matched, 1 | 2);
//! ```
//!
//! Note that `wait_*` spin until the flags appear, so a flag must be
//! set before (or concurrently with) the wait. Use [`Event::try_wait`]
//! when you want a non-blocking poll.

use crate::spin::SpinLock;

/// Event flags for task synchronization
pub struct Event {
    inner: SpinLock<EventInner>,
}

impl Default for Event {
    fn default() -> Self {
        Self::new()
    }
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
        if flags == 0 {
            return;
        }
        self.inner.lock().waiters += 1;
        loop {
            let mut inner = self.inner.lock();
            if (inner.flags & flags) == flags {
                inner.flags &= !flags; // Clear the matched flags
                inner.waiters = inner.waiters.saturating_sub(1);
                return;
            }
            drop(inner);
            core::hint::spin_loop();
        }
    }

    /// Wait for any of the specified flags to be set
    pub fn wait_any(&self, flags: u32) -> u32 {
        if flags == 0 {
            return 0;
        }
        self.inner.lock().waiters += 1;
        loop {
            let mut inner = self.inner.lock();
            let matched = inner.flags & flags;
            if matched != 0 {
                inner.flags &= !matched;
                inner.waiters = inner.waiters.saturating_sub(1);
                return matched;
            }
            drop(inner);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_flags_do_not_block() {
        let event = Event::new();
        event.wait_all(0);
        assert_eq!(event.wait_any(0), 0);
        assert_eq!(event.waiter_count(), 0);
    }

    #[test]
    fn wait_any_consumes_flags_and_releases_waiter() {
        let event = Event::with_flags(0b1010);
        assert_eq!(event.wait_any(0b0110), 0b0010);
        assert_eq!(event.flags(), 0b1000);
        assert_eq!(event.waiter_count(), 0);
    }

    #[test]
    fn wait_all_clears_exactly_the_requested_flags() {
        let event = Event::with_flags(0b1111);
        event.wait_all(0b0101);
        assert_eq!(event.flags(), 0b1010);
        assert_eq!(event.waiter_count(), 0);
    }
}
