//! Wake queue management
//!
//! The wake queue is shared between the executor (consumer) and all wakers
//! (producers). It is `Arc`-friendly: the internal state lives behind a
//! `SpinLock` so the same `Arc<WakeQueue>` can be cloned into every waker.

extern crate caplet_task;
use caplet_task::TaskId;
use crate::wake_source::WakeSource;
use ::caplet_sync::SpinLock;
use alloc::collections::BinaryHeap;
use alloc::sync::Arc;
use core::cmp::Ordering;
use core::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

/// Priority-ordered wake entry
#[derive(Debug, Clone)]
pub struct WakeEntry {
    /// Task ID to wake
    pub task_id: TaskId,
    /// Task priority
    pub priority: u8,
    /// Sequence number (for FIFO ordering at same priority)
    pub seq: u64,
    /// Source of the wake event
    pub source: WakeSource,
}

impl WakeEntry {
    /// Create a new wake entry
    pub fn new(task_id: TaskId, priority: u8, source: WakeSource) -> Self {
        Self {
            task_id,
            priority,
            seq: next_seq(),
            source,
        }
    }
}

impl PartialEq for WakeEntry {
    fn eq(&self, other: &Self) -> bool {
        self.seq == other.seq
    }
}

impl Eq for WakeEntry {}

impl PartialOrd for WakeEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WakeEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Lower priority value = higher priority
        other.priority.cmp(&self.priority)
            .then(self.seq.cmp(&other.seq))
    }
}

/// Counter split across two `AtomicU32`s so we work on targets without
/// `AtomicU64` (e.g. ARMv6-M).
static SEQ_HI: AtomicU32 = AtomicU32::new(0);
static SEQ_LO: AtomicU32 = AtomicU32::new(0);

fn next_seq() -> u64 {
    let lo = SEQ_LO.fetch_add(1, AtomicOrdering::Relaxed);
    let hi = if lo == u32::MAX { SEQ_HI.fetch_add(1, AtomicOrdering::Relaxed) } else { 0 };
    ((hi as u64) << 32) | (lo as u64)
}

/// Queue of pending wake-ups.
///
/// Shared between the executor and all wakers via `Arc<WakeQueue>`. Operations
/// take a brief spin lock to coordinate the underlying `BinaryHeap`.
pub struct WakeQueue {
    inner: SpinLock<BinaryHeap<WakeEntry>>,
}

impl core::fmt::Debug for WakeQueue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let g = self.inner.lock();
        f.debug_struct("WakeQueue").field("len", &g.len()).finish()
    }
}

impl WakeQueue {
    /// Create a new empty wake queue
    pub const fn new() -> Self {
        Self {
            inner: SpinLock::new(BinaryHeap::new()),
        }
    }

    /// Add a wake entry (cheap — just acquires the spin lock briefly).
    pub fn push(&self, entry: WakeEntry) {
        self.inner.lock().push(entry);
    }

    /// Pop the highest priority entry.
    pub fn pop(&self) -> Option<WakeEntry> {
        self.inner.lock().pop()
    }

    /// Drain all currently-queued entries into a `Vec`, returning them in
    /// the order they should be polled (highest priority first). The queue
    /// is left empty.
    pub fn take_ready_snapshot(&self) -> alloc::vec::Vec<WakeEntry> {
        let mut guard = self.inner.lock();
        let mut out = alloc::vec::Vec::with_capacity(guard.len());
        while let Some(e) = guard.pop() {
            out.push(e);
        }
        out
    }

    /// Peek at the highest priority entry (returns a clone to release the lock).
    pub fn peek(&self) -> Option<WakeEntry> {
        self.inner.lock().peek().cloned()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.inner.lock().clear();
    }
}

/// Convenience: a `Shared<WakeQueue>` is an `Arc<WakeQueue>` that is
/// cheap to clone into each waker.
pub type SharedWakeQueue = Arc<WakeQueue>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wake_source::WakeSource;

    #[test]
    fn test_push_pop_priority() {
        let q = WakeQueue::new();
        q.push(WakeEntry::new(TaskId::new(1), 5, WakeSource::Manual));
        q.push(WakeEntry::new(TaskId::new(2), 1, WakeSource::Manual));
        q.push(WakeEntry::new(TaskId::new(3), 3, WakeSource::Manual));
        let e1 = q.pop().unwrap();
        assert_eq!(e1.task_id.as_u32(), 2); // priority 1 first
        let e2 = q.pop().unwrap();
        assert_eq!(e2.task_id.as_u32(), 3);
        let e3 = q.pop().unwrap();
        assert_eq!(e3.task_id.as_u32(), 1);
    }

    #[test]
    fn test_take_ready_snapshot() {
        let q = WakeQueue::new();
        q.push(WakeEntry::new(TaskId::new(1), 5, WakeSource::Initial));
        q.push(WakeEntry::new(TaskId::new(2), 1, WakeSource::Initial));
        let snap = q.take_ready_snapshot();
        assert_eq!(snap.len(), 2);
        assert!(q.is_empty());
        // Highest priority first.
        assert_eq!(snap[0].task_id.as_u32(), 2);
    }
}
