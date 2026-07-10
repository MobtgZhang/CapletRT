//! Hierarchical timer wheel.
//!
//! A timer wheel stores pending timers in `BUCKETS` circular queues
//! of `SLOTS` entries each. The current "tick" advances once per
//! millisecond (configurable). A timer that should fire in `n`
//! ticks lands in bucket `floor(n/SLOTS) % BUCKETS` at slot
//! `n % SLOTS` within that bucket.
//!
//! Insertion is `O(1)` (no list walk). Firing is `O(1)` amortised
//! per timer; per-tick amortised cost is `O(K)` where K is the
//! number of timers in the current slot — usually tiny.
//!
//! The wheel is *fixed-capacity*: each slot holds up to 16 timers.
//! Excess timers are dropped with the [`TimerError::WheelFull`]
//! error.

extern crate alloc;

use alloc::collections::vec_deque::VecDeque;
use core::sync::atomic::{AtomicU64, Ordering};

/// Number of buckets (one per `SLOTS`-tick window).
pub const BUCKETS: usize = 16;
/// Entries per bucket.
pub const SLOTS: usize = 8;

/// Errors returned by timer scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    /// Wheel is at capacity; the new timer was not inserted.
    WheelFull,
    /// Deadline already in the past.
    PastDeadline,
    /// Wheel is at `0` ticks; wheel has not been started yet.
    NotStarted,
}

/// Identifier for an installed timer. Stable across
/// cancellations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimerId(pub u32);

/// Opaque payload returned to the consumer when a timer fires.
pub type Token = u64;

struct TimerEntry {
    /// Stable identifier.
    id: TimerId,
    /// Absolute deadline in ticks.
    deadline: u64,
    /// Per-timer payload token.
    token: Token,
}

/// Pending timer list per bucket slot.
type BucketSlot = VecDeque<TimerEntry>;

/// Timer wheel.
pub struct TimerWheel {
    /// Buckets[slide position][slot] -> timer queue.
    buckets: [[BucketSlot; SLOTS]; BUCKETS],
    /// Current tick counter.
    ticks: AtomicU64,
    /// Monotonic ID counter for `TimerId::new`.
    next_id: AtomicU64,
}

impl TimerWheel {
    /// Construct an empty wheel.
    pub const fn new() -> Self {
        Self {
            buckets: [const { [const { BucketSlot::new() }; SLOTS] }; BUCKETS],
            ticks: AtomicU64::new(0),
            next_id: AtomicU64::new(0),
        }
    }

    /// Current tick count.
    pub fn now(&self) -> u64 {
        self.ticks.load(Ordering::Acquire)
    }

    /// Allocate the next timer ID.
    fn alloc_id(&self) -> TimerId {
        TimerId(self.next_id.fetch_add(1, Ordering::AcqRel) as u32)
    }

    /// Schedule a one-shot timer to fire `after_ticks` from "now".
    ///
    /// # Errors
    /// * `TimerError::PastDeadline` if `after_ticks == 0`.
    /// * `TimerError::WheelFull` if no slot is free.
    pub fn schedule(&mut self, after_ticks: u64, token: Token) -> Result<TimerId, TimerError> {
        if after_ticks == 0 {
            return Err(TimerError::PastDeadline);
        }
        let now = self.now();
        let deadline = now.checked_add(after_ticks).ok_or(TimerError::PastDeadline)?;
        let bucket = (deadline / SLOTS as u64) as usize % BUCKETS;
        let slot = (deadline % SLOTS as u64) as usize;
        let id = self.alloc_id();
        if self.buckets[bucket][slot].len() >= 16 {
            return Err(TimerError::WheelFull);
        }
        self.buckets[bucket][slot].push_back(TimerEntry {
            id,
            deadline,
            token,
        });
        Ok(id)
    }

    /// Cancel a previously-scheduled timer. Returns `true` if it
    /// was located and removed.
    pub fn cancel(&mut self, id: TimerId) -> bool {
        for bucket in self.buckets.iter_mut() {
            for slot in bucket.iter_mut() {
                if let Some(pos) = slot.iter().position(|e| e.id == id) {
                    slot.remove(pos);
                    return true;
                }
            }
        }
        false
    }

    /// Advance the wheel by exactly one tick. Returns the list of
    /// timer tokens that fire at this tick, in the order they
    /// appear in the wheel.
    pub fn tick(&mut self) -> alloc::vec::Vec<Token> {
        let now = self.ticks.fetch_add(1, Ordering::AcqRel) + 1;
        let bucket = (now / SLOTS as u64) as usize % BUCKETS;
        let slot = (now % SLOTS as u64) as usize;
        let mut out = alloc::vec::Vec::new();
        for entry in self.buckets[bucket][slot].drain(..) {
            out.push(entry.token);
        }
        out
    }

    /// Count currently scheduled timers (testing helper).
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.buckets.iter().flatten().map(|s| s.len()).sum()
    }
}

impl Default for TimerWheel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn schedule_fires_at_right_tick() {
        let mut w = TimerWheel::new();
        let id = w.schedule(3, 42).unwrap();
        assert_eq!(w.tick(), vec![]);
        assert_eq!(w.tick(), vec![]);
        let fired = w.tick();
        assert_eq!(fired, vec![42]);
        assert_eq!(w.len(), 0);
        // Cancelling an already-fired timer is a no-op.
        assert!(!w.cancel(id));
    }

    #[test]
    fn cancel_returns_true_only_for_known_id() {
        let mut w = TimerWheel::new();
        let id = w.schedule(2, 1).unwrap();
        assert!(w.cancel(id));
        assert!(!w.cancel(id));
    }

    #[test]
    fn zero_deadline_rejected() {
        let mut w = TimerWheel::new();
        assert_eq!(w.schedule(0, 0), Err(TimerError::PastDeadline));
    }

    #[test]
    fn wheel_wraps_around_buckets() {
        let mut w = TimerWheel::new();
        // Schedule something that straddles the bucket boundary.
        let _ = w.schedule(SLOTS as u64 * 2 + 1, 0xDEAD).unwrap();
        // Bump enough ticks to wrap past the bucket boundary.
        for _ in 0..(SLOTS * 2) {
            w.tick();
        }
        let fired = w.tick();
        assert_eq!(fired, vec![0xDEAD]);
    }
}