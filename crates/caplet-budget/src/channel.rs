//! Budget-rejection event channel.
//!
//! Whenever a `BudgetTracker::charge_*` call fails, the calling
//! task's [`RejectionSink`] is given a chance to publish a
//! [`RejectionEvent`]. A supervisor task subscribed to the
//! [`RejectionChannel`] receives every event so it can react
//! (kill the offending task, log, file a report, etc.).
//!
//! The channel is single-producer-multi-consumer using the
//! `caplet-sync::Event` primitive plus an 8-deep ring buffer
//! of rejection records. The buffer guarantees at-most-once
//! delivery per subscriber per record; past-overflow records
//! are dropped with a counter. This matches the M2 requirement:
//! never block the producer on a slow consumer.

extern crate alloc;

use crate::{BudgetError, BudgetTracker, MicroSeconds};
use alloc::sync::Arc;
use caplet_sync::event::Event;

/// Bit-flag identifying the source of a rejection.
pub mod src {
    /// Memory budget exhaustion.
    pub const MEMORY: u32 = 1 << 0;
    /// CPU per-quantum exhaustion.
    pub const CPU: u32 = 1 << 1;
    /// Message-rate per-second exhaustion.
    pub const MESSAGE: u32 = 1 << 2;
    /// Combined flag-set.
    pub const ANY: u32 = MEMORY | CPU | MESSAGE;
}

/// A single budget-rejection record.
#[derive(Debug, Clone, Copy)]
pub struct RejectionEvent {
    /// Which kind of rejection this is.
    pub source: u32,
    /// Task ID (`caplet_task::TaskId::as_u32()`).
    pub task: u32,
    /// Snapshot of the budget error at the moment of rejection.
    pub reason: BudgetError,
}

/// Ring buffer for the rejection channel.
const RING_CAP: usize = 8;

struct Ring {
    slots: [RejectionEvent; RING_CAP],
    head: usize,
    tail: usize,
    dropped: u32,
}

impl Ring {
    const fn new() -> Self {
        Self {
            slots: [RejectionEvent {
                source: 0,
                task: 0,
                reason: BudgetError::MessageRateExceeded {
                    used: 0,
                    limit: 0,
                },
            }; RING_CAP],
            head: 0,
            tail: 0,
            dropped: 0,
        }
    }
}

/// Single-producer broadcast channel.
pub struct RejectionChannel {
    inner: caplet_sync::spin::SpinLock<Ring>,
    /// Event flag set whenever a new record is published.
    event: Event,
}

impl RejectionChannel {
    /// Create a new channel.
    pub const fn new() -> Self {
        Self {
            inner: caplet_sync::spin::SpinLock::new(Ring::new()),
            event: Event::new(),
        }
    }

    /// Publish a rejection event. Always non-blocking; older
    /// records are overwritten if the ring is full and the
    /// `dropped` counter is incremented.
    pub fn publish(&self, ev: RejectionEvent) {
        let mut inner = self.inner.lock();
        // Capacity = RING_CAP - 1 slots. Full == next head slot
        // matches tail. When that happens drop the oldest
        // record first.
        let next_head = (inner.head + 1) % RING_CAP;
        if next_head == inner.tail {
            inner.tail = (inner.tail + 1) % RING_CAP;
            inner.dropped += 1;
        }
        let head = inner.head;
        inner.slots[head] = ev;
        inner.head = next_head;
        drop(inner);
        self.event.set(src::ANY);
    }

    /// Drain every available record. Returns an iterator-like
    /// bounded list (or `None` if empty). Consumer resets the
    /// event flag after each drain.
    pub fn drain(&self) -> alloc::vec::Vec<RejectionEvent> {
        let mut out = alloc::vec::Vec::new();
        {
            let mut inner = self.inner.lock();
            while inner.tail != inner.head {
                out.push(inner.slots[inner.tail]);
                inner.tail = (inner.tail + 1) % RING_CAP;
            }
            inner.dropped = 0;
        }
        self.event.clear(src::ANY);
        out
    }

    #[cfg(test)]
    fn dropped(&self) -> u32 {
        self.inner.lock().dropped
    }

    /// Wait (spin) until *any* source has produced an event,
    /// then return the drain list.
    pub fn wait(&self) -> alloc::vec::Vec<RejectionEvent> {
        self.event.wait_any(src::ANY);
        self.drain()
    }

    /// Subscribe a [`RejectionSink`] to this channel.
    pub fn subscribe(self: &Arc<Self>) -> RejectionSink {
        RejectionSink { channel: self.clone() }
    }
}

/// Per-task sink. Cheap to clone; can be embedded in any task.
#[derive(Clone)]
pub struct RejectionSink {
    channel: Arc<RejectionChannel>,
}

impl RejectionSink {
    /// Report a rejection from the given task. Convenience over
    /// [`RejectionChannel::publish`].
    pub fn report(&self, task_id: u32, err: BudgetError) {
        let source = match err {
            BudgetError::MemoryExceeded { .. } => src::MEMORY,
            BudgetError::CpuExceeded { .. } => src::CPU,
            BudgetError::MessageRateExceeded { .. } => src::MESSAGE,
        };
        self.channel.publish(RejectionEvent {
            source,
            task: task_id,
            reason: err,
        });
    }
}

/// Wrap a [`BudgetTracker`] so that *any* charge that would
/// exceed the budget is also reported into the sink.
pub struct TrackedBudget {
    /// Underlying budget tracker.
    pub tracker: BudgetTracker,
    /// Channel sink for reporting rejections.
    pub sink: RejectionSink,
    /// Owning task ID.
    pub task_id: u32,
}

impl TrackedBudget {
    /// Create a new tracked budget.
    pub fn new(tracker: BudgetTracker, sink: RejectionSink, task_id: u32) -> Self {
        Self { tracker, sink, task_id }
    }

    /// Try to charge memory, reporting on failure.
    pub fn charge_memory(&self, n: u32) -> Result<(), BudgetError> {
        if let Err(e) = self.tracker.charge_memory(n) {
            self.sink.report(self.task_id, e);
            return Err(e);
        }
        Ok(())
    }

    /// Try to charge CPU, reporting on failure.
    pub fn charge_cpu(&self, us: MicroSeconds) -> Result<(), BudgetError> {
        if let Err(e) = self.tracker.charge_cpu(us) {
            self.sink.report(self.task_id, e);
            return Err(e);
        }
        Ok(())
    }

    /// Try to charge a message slot, reporting on failure.
    pub fn charge_message(&self) -> Result<(), BudgetError> {
        if let Err(e) = self.tracker.charge_message() {
            self.sink.report(self.task_id, e);
            return Err(e);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Budget, BudgetTracker};

    #[test]
    fn publish_then_drain() {
        let channel = Arc::new(RejectionChannel::new());
        channel.publish(RejectionEvent {
            source: src::MEMORY,
            task: 7,
            reason: BudgetError::MemoryExceeded {
                requested: 100,
                used: 50,
                limit: 1,
            },
        });
        assert_eq!(channel.drain().len(), 1);
        assert!(channel.drain().is_empty());
    }

    #[test]
    fn overflow_is_counted() {
        let channel = Arc::new(RejectionChannel::new());
        // RING_CAP = 8 -> 7 usable slots (one stays empty as
        // sentinel). Publishing 12 events should yield 7
        // records + 5 drops.
        for _ in 0..(RING_CAP + 4) {
            channel.publish(RejectionEvent {
                source: src::CPU,
                task: 1,
                reason: BudgetError::CpuExceeded {
                    requested: 1,
                    used: 1,
                    limit: 1,
                },
            });
        }
        let dropped_before = channel.dropped();
        let drained = channel.drain();
        assert_eq!(drained.len(), RING_CAP - 1);
        assert_eq!(dropped_before, 5);
    }

    #[test]
    fn tracked_budget_reports_on_failure() {
        let channel = Arc::new(RejectionChannel::new());
        let sink = channel.subscribe();
        let tracker = BudgetTracker::new(Budget {
            memory_bytes: 16,
            ..Budget::default()
        });
        let tb = TrackedBudget::new(tracker, sink, 42);
        assert!(tb.charge_memory(8).is_ok());
        assert!(tb.charge_memory(16).is_err());
        let drained = channel.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].task, 42);
        assert_eq!(drained[0].source, src::MEMORY);
    }
}