//! Per-task resource budgets.
//!
//! A task is *configured* with a [`Budget`] describing the most
//! of each resource class it may consume, and *tracked* by a
//! [`BudgetTracker`] that records the current consumption. The
//! kernel calls [`BudgetTracker::charge_*`] before allocating
//! anything; each charge either succeeds or produces a
//! [`BudgetError`].
//!
//! All arithmetic is done in saturating arithmetic so that a
//! runaway task cannot overflow the tracker into a state where
//! it under-reports usage.

#![no_std]
#![deny(missing_docs)]

extern crate alloc;

pub mod channel;

use core::sync::atomic::{AtomicU32, Ordering};

/// Memory budget (heap bytes). 32-bit is enough for any
/// Cortex-M target since the address space itself is 32-bit.
pub type Bytes = u32;

/// CPU budget expressed in microseconds. On Cortex-M, 32-bit allows
/// ~4294 seconds (71 minutes) before overflow. For longer durations,
/// use a 64-bit counter on other targets.
#[cfg(target_arch = "arm")]
pub type MicroSeconds = u32;
/// CPU budget expressed in microseconds. On non-ARM targets, 64-bit
/// allows ~580_000 years before overflow.
#[cfg(not(target_arch = "arm"))]
pub type MicroSeconds = u64;

/// Messages counted (both sent and received).
#[cfg(target_arch = "arm")]
pub type MessageCount = u32;
/// Messages counted (both sent and received). On non-ARM targets,
/// 64-bit allows ~580_000 years before overflow.
#[cfg(not(target_arch = "arm"))]
pub type MessageCount = u64;

/// All three resource classes in one constant record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Maximum heap bytes the task may own.
    pub memory_bytes: Bytes,
    /// Maximum CPU time the task may consume per quantum.
    pub cpu_us_per_quantum: MicroSeconds,
    /// Maximum messages the task may exchange per second.
    pub messages_per_sec: MessageCount,
}

impl Budget {
    /// "Unlimited" budget — convenience for tests and the
    /// privileged main task. The numeric values are chosen
    /// large enough that no real Cortex-M task could exceed
    /// them.
    pub const UNLIMITED: Self = Self {
        memory_bytes: Bytes::MAX,
        cpu_us_per_quantum: MicroSeconds::MAX,
        messages_per_sec: MessageCount::MAX,
    };

    /// `true` if *all* budget entries are saturated.
    pub fn is_unlimited(&self) -> bool {
        *self == Self::UNLIMITED
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            memory_bytes: 1024,
            cpu_us_per_quantum: 10_000,
            messages_per_sec: 100,
        }
    }
}

/// Reason a budget charge was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetError {
    /// Memory request exceeds the task's heap budget.
    MemoryExceeded {
        /// Requested amount.
        requested: Bytes,
        /// Currently in use.
        used: Bytes,
        /// Hard limit.
        limit: Bytes,
    },
    /// CPU charge would exceed the per-quantum limit.
    CpuExceeded {
        /// Requested amount.
        requested: MicroSeconds,
        /// Currently in use.
        used: MicroSeconds,
        /// Hard limit.
        limit: MicroSeconds,
    },
    /// Message rate would exceed the per-second limit.
    MessageRateExceeded {
        /// Current count over the rolling 1-second window.
        used: MessageCount,
        /// Hard limit.
        limit: MessageCount,
    },
}

impl BudgetError {
    /// "Worst"-picks from two errors so that the caller can
    /// record a single error reason per charge.
    pub fn merge(self, other: BudgetError) -> BudgetError {
        // Memory errors are more severe (could OOM the kernel),
        // then message rate (DoS), then CPU (just lag).
        match (self, other) {
            (BudgetError::MemoryExceeded { .. }, _) => self,
            (_, BudgetError::MemoryExceeded { .. }) => other,
            (BudgetError::MessageRateExceeded { .. }, _) => self,
            (_, BudgetError::MessageRateExceeded { .. }) => other,
            (BudgetError::CpuExceeded { .. }, BudgetError::CpuExceeded { .. }) => self,
            // All other combinations are covered by the above.
            // If we somehow reach here, return self as fallback.
            (BudgetError::CpuExceeded { .. }, _) => self,
        }
    }
}

/// Per-task live tracker. All fields are atomics so that the
/// budget can be probed from interrupts / the syscall handler
/// without taking the kernel lock.
#[derive(Debug)]
pub struct BudgetTracker {
    /// Heap bytes currently owned.
    memory_used: AtomicU32,
    /// CPU microseconds used in the current quantum.
    #[cfg(target_arch = "arm")]
    cpu_used: AtomicU32,
    #[cfg(not(target_arch = "arm"))]
    cpu_used: AtomicCounter,
    /// Messages exchanged in the current 1-second window.
    #[cfg(target_arch = "arm")]
    messages_used: AtomicU32,
    #[cfg(not(target_arch = "arm"))]
    messages_used: AtomicCounter,
    /// Hard limits. Set once during construction; effectively
    /// immutable after that so no atomics are needed.
    pub limits: Budget,
}

impl BudgetTracker {
    /// Create a new tracker.
    pub fn new(budget: Budget) -> Self {
        Self {
            memory_used: AtomicU32::new(0),
            #[cfg(target_arch = "arm")]
            cpu_used: AtomicU32::new(0),
            #[cfg(not(target_arch = "arm"))]
            cpu_used: AtomicCounter::new(0),
            #[cfg(target_arch = "arm")]
            messages_used: AtomicU32::new(0),
            #[cfg(not(target_arch = "arm"))]
            messages_used: AtomicCounter::new(0),
            limits: budget,
        }
    }

    /// Try to charge `n` bytes to the memory budget.
    pub fn charge_memory(&self, n: Bytes) -> Result<(), BudgetError> {
        loop {
            let used = self.memory_used.load(Ordering::Relaxed);
            let next = used.saturating_add(n);
            if next > self.limits.memory_bytes {
                return Err(BudgetError::MemoryExceeded {
                    requested: n,
                    used,
                    limit: self.limits.memory_bytes,
                });
            }
            if self
                .memory_used
                .compare_exchange(used, next, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    /// Release `n` bytes (refund).
    pub fn refund_memory(&self, n: Bytes) {
        self.memory_used.fetch_sub(n.min(self.memory_used.load(Ordering::Relaxed)), Ordering::AcqRel);
    }

    /// Try to charge `us` microseconds to the CPU budget.
    pub fn charge_cpu(&self, us: MicroSeconds) -> Result<(), BudgetError> {
        loop {
            let used = self.cpu_used.load(Ordering::Relaxed);
            let next = used.saturating_add(us);
            if next > self.limits.cpu_us_per_quantum {
                return Err(BudgetError::CpuExceeded {
                    requested: us,
                    used,
                    limit: self.limits.cpu_us_per_quantum,
                });
            }
            if self
                .cpu_used
                .compare_exchange(used, next, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    /// Reset the CPU accumulator (called at quantum boundary).
    pub fn reset_cpu(&self) {
        self.cpu_used.store(0, Ordering::Release);
    }

    /// Try to charge one message slot.
    pub fn charge_message(&self) -> Result<(), BudgetError> {
        loop {
            let used = self.messages_used.load(Ordering::Relaxed);
            let next = used.saturating_add(1);
            if next > self.limits.messages_per_sec {
                return Err(BudgetError::MessageRateExceeded {
                    used,
                    limit: self.limits.messages_per_sec,
                });
            }
            if self
                .messages_used
                .compare_exchange(used, next, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    /// Reset the message rate (called every second).
    pub fn reset_messages(&self) {
        self.messages_used.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_budget_accepts_huge_charge() {
        let t = BudgetTracker::new(Budget::UNLIMITED);
        assert!(t.charge_memory(Bytes::MAX / 2).is_ok());
    }

    #[test]
    fn memory_budget_enforced() {
        let t = BudgetTracker::new(Budget {
            memory_bytes: 1024,
            ..Budget::default()
        });
        assert!(t.charge_memory(900).is_ok());
        assert!(t.charge_memory(200).is_err());
        // Refund restores capacity.
        t.refund_memory(900);
        assert!(t.charge_memory(900).is_ok());
    }

    #[test]
    fn cpu_budget_clamps_at_limit() {
        let t = BudgetTracker::new(Budget {
            cpu_us_per_quantum: 1_000,
            ..Budget::default()
        });
        assert!(t.charge_cpu(700).is_ok());
        // Saturating: 700 + 400 = 1100 exceeds, should fail.
        assert!(t.charge_cpu(400).is_err());
        t.reset_cpu();
        assert!(t.charge_cpu(700).is_ok());
    }

    #[test]
    fn message_rate_bounded_by_window() {
        let t = BudgetTracker::new(Budget {
            messages_per_sec: 3,
            ..Budget::default()
        });
        assert!(t.charge_message().is_ok());
        assert!(t.charge_message().is_ok());
        assert!(t.charge_message().is_ok());
        assert!(t.charge_message().is_err());
        t.reset_messages();
        assert!(t.charge_message().is_ok());
    }

    #[test]
    fn merge_picks_memory_first() {
        let mem = BudgetError::MemoryExceeded {
            requested: 10,
            used: 5,
            limit: 1,
        };
        let cpu = BudgetError::CpuExceeded {
            requested: 10,
            used: 5,
            limit: 1,
        };
        let merged = mem.merge(cpu);
        assert!(matches!(merged, BudgetError::MemoryExceeded { .. }));
    }
}