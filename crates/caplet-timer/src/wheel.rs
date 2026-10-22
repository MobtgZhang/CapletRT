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
use core::sync::atomic::Ordering;

// `thumbv7em-none-eabihf` (Cortex-M4/M7) has no 64-bit atomics.  The
// counter only needs to be atomic so that `TimerWheel` can live in a
// `static`; every mutating operation already takes `&mut self`.  Use a
// 32-bit counter on targets without `AtomicU64` and widen it at the API
// boundary.  This keeps the crate usable on every bare-metal target the
// project supports while preserving 64-bit behaviour on 64-bit hosts.
#[cfg(not(target_has_atomic = "64"))]
use core::sync::atomic::AtomicU32;
#[cfg(target_has_atomic = "64")]
use core::sync::atomic::AtomicU64;

#[cfg(target_has_atomic = "64")]
type AtomicCounter = AtomicU64;
#[cfg(not(target_has_atomic = "64"))]
type AtomicCounter = AtomicU32;

#[inline]
const fn counter_new(value: u64) -> AtomicCounter {
    #[cfg(target_has_atomic = "64")]
    {
        AtomicCounter::new(value)
    }
    #[cfg(not(target_has_atomic = "64"))]
    {
        AtomicCounter::new(value as u32)
    }
}

#[inline]
fn counter_load(counter: &AtomicCounter) -> u64 {
    #[cfg(target_has_atomic = "64")]
    {
        counter.load(Ordering::Acquire)
    }
    #[cfg(not(target_has_atomic = "64"))]
    {
        counter.load(Ordering::Acquire) as u64
    }
}

#[inline]
fn counter_fetch_add(counter: &AtomicCounter, value: u64) -> u64 {
    #[cfg(target_has_atomic = "64")]
    {
        counter.fetch_add(value, Ordering::AcqRel)
    }
    #[cfg(not(target_has_atomic = "64"))]
    {
        counter.fetch_add(value as u32, Ordering::AcqRel) as u64
    }
}

#[inline]
fn counter_store(counter: &AtomicCounter, value: u64) {
    #[cfg(target_has_atomic = "64")]
    {
        counter.store(value, Ordering::Release);
    }
    #[cfg(not(target_has_atomic = "64"))]
    {
        counter.store(value as u32, Ordering::Release);
    }
}

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
    /// Per-timer payload token.
    token: Token,
    /// Absolute tick at which this timer becomes eligible to fire.
    ///
    /// The bucket index is derived from the deadline modulo the wheel
    /// geometry, so two timers more than one wheel revolution apart can
    /// share a slot.  Keeping the absolute deadline lets `tick` leave the
    /// later timer queued instead of firing it during an earlier revolution.
    deadline: u64,
}

/// Pending timer list per bucket slot.
type BucketSlot = VecDeque<TimerEntry>;

/// Timer wheel.
pub struct TimerWheel {
    /// Buckets[slide position][slot] -> timer queue.
    buckets: [[BucketSlot; SLOTS]; BUCKETS],
    /// Current tick counter.
    ticks: AtomicCounter,
    /// Monotonic ID counter for `TimerId::new`.
    next_id: AtomicCounter,
}

impl TimerWheel {
    /// Construct an empty wheel.
    pub const fn new() -> Self {
        Self {
            buckets: [const { [const { BucketSlot::new() }; SLOTS] }; BUCKETS],
            ticks: counter_new(0),
            next_id: counter_new(0),
        }
    }

    /// Current tick count.
    pub fn now(&self) -> u64 {
        counter_load(&self.ticks)
    }

    /// Allocate the next timer ID.
    fn alloc_id(&self) -> TimerId {
        TimerId(counter_fetch_add(&self.next_id, 1) as u32)
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
        let deadline = now
            .checked_add(after_ticks)
            .ok_or(TimerError::PastDeadline)?;
        self.schedule_at(deadline, token)
    }

    /// Schedule a one-shot timer for an absolute `deadline`.
    ///
    /// A deadline at or before the current tick is placed in the
    /// current slot and is reported by the next [`Self::advance_to`]
    /// call for the current time (or by [`Self::tick`]). Unlike
    /// [`Self::schedule`], this is the entry point used by absolute
    /// timer syscalls and therefore accepts an already-due deadline.
    pub fn schedule_at(&mut self, deadline: u64, token: Token) -> Result<TimerId, TimerError> {
        let now = self.now();
        // A past deadline is already due; clamping it to now keeps the
        // wheel geometry valid without losing the wakeup.
        let deadline = deadline.max(now);
        let bucket = (deadline / SLOTS as u64) as usize % BUCKETS;
        let slot = (deadline % SLOTS as u64) as usize;
        let id = self.alloc_id();
        if self.buckets[bucket][slot].len() >= 16 {
            return Err(TimerError::WheelFull);
        }
        self.buckets[bucket][slot].push_back(TimerEntry {
            id,
            token,
            deadline,
        });
        Ok(id)
    }

    /// Return the earliest scheduled absolute deadline, if any.
    pub fn next_deadline(&self) -> Option<u64> {
        self.buckets
            .iter()
            .flatten()
            .flat_map(|slot| slot.iter())
            .map(|entry| entry.deadline)
            .min()
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
        let now = counter_fetch_add(&self.ticks, 1) + 1;
        self.collect_due(now)
    }

    /// Advance to an absolute tick, returning every timer whose
    /// deadline falls in `(now, target]`. Calling this with the
    /// current time drains timers that were scheduled with an
    /// already-due absolute deadline.
    pub fn advance_to(&mut self, target: u64) -> alloc::vec::Vec<Token> {
        let mut out = alloc::vec::Vec::new();
        if target < self.now() {
            return out;
        }
        // Jumping directly to the target is safe: every entry stores an
        // absolute deadline, so a single pass over the fixed number of
        // slots can drain all timers due anywhere in the skipped range.
        // This keeps an absolute-clock jump (for example after a large
        // timestamp adjustment) bounded by wheel capacity rather than
        // by the size of the time delta.
        counter_store(&self.ticks, target);
        for bucket in self.buckets.iter_mut() {
            for queue in bucket.iter_mut() {
                let mut pending = alloc::vec::Vec::new();
                while let Some(entry) = queue.pop_front() {
                    if entry.deadline <= target {
                        out.push(entry.token);
                    } else {
                        pending.push(entry);
                    }
                }
                queue.extend(pending);
            }
        }
        out
    }

    fn collect_due(&mut self, now: u64) -> alloc::vec::Vec<Token> {
        let bucket = (now / SLOTS as u64) as usize % BUCKETS;
        let slot = (now % SLOTS as u64) as usize;
        let mut out = alloc::vec::Vec::new();
        let queue = &mut self.buckets[bucket][slot];
        let mut pending = alloc::vec::Vec::new();
        while let Some(entry) = queue.pop_front() {
            if entry.deadline <= now {
                out.push(entry.token);
            } else {
                // A long delay aliases an earlier revolution of the wheel.
                // Keep it parked until its absolute deadline arrives.
                pending.push(entry);
            }
        }
        queue.extend(pending);
        out
    }

    /// Count currently scheduled timers (testing helper).
    pub fn len(&self) -> usize {
        self.buckets.iter().flatten().map(|s| s.len()).sum()
    }

    /// Return `true` when no timers are scheduled.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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

    #[test]
    fn long_deadline_does_not_fire_during_earlier_revolution() {
        let mut w = TimerWheel::new();
        let revolution = (BUCKETS * SLOTS) as u64;
        w.schedule(revolution + 1, 0xA11CE).unwrap();

        for _ in 0..revolution {
            assert!(w.tick().is_empty());
        }
        assert_eq!(w.tick(), vec![0xA11CE]);
        assert!(w.is_empty());
    }

    #[test]
    fn timers_sharing_an_aliased_slot_fire_at_their_own_deadlines() {
        let mut w = TimerWheel::new();
        let revolution = (BUCKETS * SLOTS) as u64;
        w.schedule(2, 1).unwrap();
        w.schedule(revolution + 2, 2).unwrap();

        assert!(w.tick().is_empty());
        assert_eq!(w.tick(), vec![1]);
        for _ in 2..(revolution + 1) {
            assert!(w.tick().is_empty());
        }
        assert_eq!(w.tick(), vec![2]);
    }

    #[test]
    fn absolute_deadline_at_now_fires_without_advancing() {
        let mut w = TimerWheel::new();
        w.schedule_at(0, 7).unwrap();
        assert_eq!(w.next_deadline(), Some(0));
        assert_eq!(w.advance_to(0), vec![7]);
        assert!(w.is_empty());
        assert_eq!(w.next_deadline(), None);
    }

    #[test]
    fn advance_to_reports_every_intermediate_timer() {
        let mut w = TimerWheel::new();
        w.schedule(2, 2).unwrap();
        w.schedule(4, 4).unwrap();
        w.schedule(9, 9).unwrap();
        assert_eq!(w.advance_to(4), vec![2, 4]);
        assert_eq!(w.next_deadline(), Some(9));
        assert_eq!(w.advance_to(9), vec![9]);
    }
}
