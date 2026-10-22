//! Kernel resource-quota facade.
//!
//! [`caplet_budget`] provides the lock-free accounting primitives.  This
//! module binds them to a process, maps the descriptive [`QuotaSet`] used
//! by task manifests onto a [`Budget`], and implements the one-second
//! epoch used by the message and CPU quotas.
//!
//! Charging helpers deliberately treat "no current process" as success:
//! the boot task, kernel threads and host-side syscall tests are
//! privileged and are not subject to a user-process quota.

use caplet_budget::channel::{RejectionChannel, RejectionSink};
use caplet_budget::{Budget, BudgetError, BudgetTracker, Bytes, MicroSeconds};
use core::sync::atomic::{AtomicU32, Ordering};

/// Length of one quota epoch, in milliseconds.
pub const EPOCH_MS: u32 = 1_000;

/// Fixed accounting cost of one capability slot.
pub const CAPABILITY_SLOT_BYTES: Bytes = 64;

/// Human-readable quota limits used by task manifests and board config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaSet {
    /// Heap bytes owned by the task.
    pub memory_bytes: Bytes,
    /// CPU microseconds per one-second epoch.
    pub cpu_us_per_epoch: MicroSeconds,
    /// Messages per one-second epoch.
    pub messages_per_epoch: u32,
}

impl QuotaSet {
    /// Default quota matching [`Budget::default`].
    pub const DEFAULT: Self = Self {
        memory_bytes: 1024,
        cpu_us_per_epoch: 10_000,
        messages_per_epoch: 100,
    };

    /// A quota that never rejects a realistic bare-metal workload.
    pub const UNLIMITED: Self = Self {
        memory_bytes: Bytes::MAX,
        cpu_us_per_epoch: MicroSeconds::MAX,
        messages_per_epoch: u32::MAX,
    };

    /// Convert to the accounting representation.
    pub const fn to_budget(self) -> Budget {
        Budget {
            memory_bytes: self.memory_bytes,
            cpu_us_per_quantum: self.cpu_us_per_epoch,
            messages_per_sec: self.messages_per_epoch as _,
        }
    }
}

impl Default for QuotaSet {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Per-process quota tracker with an optional rejection sink.
pub struct QuotaTracker {
    tracker: BudgetTracker,
    sink: Option<RejectionSink>,
    task_id: AtomicU32,
    epoch_ms: AtomicU32,
}

impl QuotaTracker {
    /// Create a tracker from the legacy [`Budget`] representation.
    pub fn new(budget: Budget) -> Self {
        Self {
            tracker: BudgetTracker::new(budget),
            sink: None,
            task_id: AtomicU32::new(0),
            epoch_ms: AtomicU32::new(0),
        }
    }

    /// Create a tracker from a manifest quota set.
    pub fn from_set(set: QuotaSet) -> Self {
        Self::new(set.to_budget())
    }

    /// Attach the supervisor channel that receives rejected charges.
    pub fn attach_rejection_channel(&mut self, channel: &alloc::sync::Arc<RejectionChannel>) {
        self.sink = Some(channel.subscribe());
    }

    /// Set the task id stamped into rejection events.
    pub fn set_task_id(&self, task_id: u32) {
        self.task_id.store(task_id, Ordering::Release);
    }

    /// Effective limits.
    pub fn limits(&self) -> Budget {
        self.tracker.limits()
    }

    /// Current heap usage.
    pub fn memory_used(&self) -> Bytes {
        self.tracker.memory_used()
    }

    /// Current CPU usage in the active epoch.
    pub fn cpu_used(&self) -> MicroSeconds {
        self.tracker.cpu_used()
    }

    /// Current message usage in the active epoch.
    pub fn messages_used(&self) -> caplet_budget::MessageCount {
        self.tracker.messages_used()
    }

    fn report(&self, error: BudgetError) {
        if let Some(sink) = &self.sink {
            sink.report(self.task_id.load(Ordering::Acquire), error);
        }
    }

    /// Charge heap bytes, reporting a rejection if the limit is exceeded.
    pub fn charge_memory(&self, bytes: Bytes) -> Result<(), BudgetError> {
        match self.tracker.charge_memory(bytes) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.report(error);
                Err(error)
            }
        }
    }

    /// Refund heap bytes after a rolled-back allocation.
    pub fn refund_memory(&self, bytes: Bytes) {
        self.tracker.refund_memory(bytes);
    }

    /// Charge CPU microseconds.
    pub fn charge_cpu(&self, us: MicroSeconds) -> Result<(), BudgetError> {
        match self.tracker.charge_cpu(us) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.report(error);
                Err(error)
            }
        }
    }

    /// Charge one message.
    pub fn charge_message(&self) -> Result<(), BudgetError> {
        match self.tracker.charge_message() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.report(error);
                Err(error)
            }
        }
    }

    /// Refund one message after a rolled-back send.
    pub fn refund_message(&self) {
        self.tracker.refund_message();
    }

    /// Advance the epoch clock, resetting CPU and message counters when
    /// a full one-second window has elapsed.
    pub fn advance_epoch(&self, ms: u32) {
        let elapsed = self.epoch_ms.fetch_add(ms, Ordering::AcqRel) + ms;
        if elapsed >= EPOCH_MS {
            self.reset_epoch();
        }
    }

    /// Start a new quota epoch immediately.
    pub fn reset_epoch(&self) {
        self.tracker.reset_cpu();
        self.tracker.reset_messages();
        self.epoch_ms.store(0, Ordering::Release);
    }
}

impl Default for QuotaTracker {
    fn default() -> Self {
        Self::new(Budget::default())
    }
}

fn with_current_quota<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&QuotaTracker) -> R,
{
    let pid = crate::kernel::current_pid()?;
    crate::kernel::with_kernel(|kernel| kernel.get_process(pid).map(|process| f(&process.quota)))
        .flatten()
}

/// Charge memory to the running task's process.  Returns `Ok(())` for
/// kernel/boot contexts with no owning process.
pub fn charge_current_memory(bytes: Bytes) -> Result<(), BudgetError> {
    with_current_quota(|quota| quota.charge_memory(bytes)).unwrap_or(Ok(()))
}

/// Refund memory previously charged to the running task.
pub fn refund_current_memory(bytes: Bytes) {
    if let Some(pid) = crate::kernel::current_pid() {
        let _ = crate::kernel::with_kernel(|kernel| {
            if let Some(process) = kernel.get_process(pid) {
                process.quota.refund_memory(bytes);
            }
        });
    }
}

/// Charge one message to the running task's process.
pub fn charge_current_message() -> Result<(), BudgetError> {
    with_current_quota(|quota| quota.charge_message()).unwrap_or(Ok(()))
}

/// Refund one message previously reserved by the running task.
pub fn refund_current_message() {
    if let Some(pid) = crate::kernel::current_pid() {
        let _ = crate::kernel::with_kernel(|kernel| {
            if let Some(process) = kernel.get_process(pid) {
                process.quota.refund_message();
            }
        });
    }
}

/// Charge CPU microseconds to the running task's process.
pub fn charge_current_cpu(us: MicroSeconds) -> Result<(), BudgetError> {
    with_current_quota(|quota| quota.charge_cpu(us)).unwrap_or(Ok(()))
}

/// Advance every process quota epoch by `ms` and charge the running
/// process for the CPU time elapsed.  Called from the timer ISR path.
#[allow(clippy::unnecessary_cast)]
pub fn tick(ms: u32) {
    let pid = crate::kernel::current_pid();
    let _ = crate::kernel::with_kernel(|kernel| {
        if let Some(pid) = pid {
            if let Some(process) = kernel.get_process_mut(pid) {
                let us = (ms as u64).saturating_mul(1_000);
                let _ = process
                    .quota
                    .charge_cpu(us.min(MicroSeconds::MAX as u64) as MicroSeconds);
            }
        }
        for process in kernel.processes.values() {
            process.quota.advance_epoch(ms);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;

    #[test]
    fn quota_set_maps_to_budget() {
        let budget = QuotaSet::DEFAULT.to_budget();
        assert_eq!(budget.memory_bytes, 1024);
        assert_eq!(budget.cpu_us_per_quantum, 10_000);
        assert_eq!(budget.messages_per_sec, 100);
    }

    #[test]
    fn messages_and_cpu_reset_after_one_second() {
        let quota = QuotaTracker::from_set(QuotaSet {
            memory_bytes: 8,
            cpu_us_per_epoch: 10,
            messages_per_epoch: 1,
        });
        assert!(quota.charge_message().is_ok());
        assert!(quota.charge_message().is_err());
        quota.refund_message();
        assert!(quota.charge_message().is_ok());
        assert!(quota.charge_cpu(10).is_ok());
        assert!(quota.charge_cpu(1).is_err());
        quota.advance_epoch(EPOCH_MS);
        assert_eq!(quota.messages_used(), 0);
        assert_eq!(quota.cpu_used(), 0);
        assert!(quota.charge_cpu(10).is_ok());
    }

    #[test]
    fn rejection_is_published_to_supervisor() {
        let channel = Arc::new(RejectionChannel::new());
        let mut quota = QuotaTracker::from_set(QuotaSet {
            memory_bytes: 4,
            cpu_us_per_epoch: 10,
            messages_per_epoch: 1,
        });
        quota.attach_rejection_channel(&channel);
        quota.set_task_id(7);
        assert!(quota.charge_memory(8).is_err());
        let events = channel.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].task, 7);
        assert!(matches!(
            events[0].reason,
            BudgetError::MemoryExceeded { .. }
        ));
    }

    #[test]
    fn memory_refund_restores_capacity() {
        let quota = QuotaTracker::from_set(QuotaSet {
            memory_bytes: 4,
            ..QuotaSet::DEFAULT
        });
        assert!(quota.charge_memory(4).is_ok());
        assert!(quota.charge_memory(1).is_err());
        quota.refund_memory(4);
        assert!(quota.charge_memory(4).is_ok());
    }
}
