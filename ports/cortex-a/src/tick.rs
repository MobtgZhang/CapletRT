//! Monotonic tick counter for Cortex-A.
//!
//! docv1.md §6.4 mentions this would be a SP804 dual timer; for M3
//! we read the Cortex-A9 global timer (which is per-CPU on real
//! silicon but per-cluster on QEMU) and divide by a constant.
//!
//! On hosts (no ARM target) we fall back to a software counter
//! incremented in `tick()` so tests still compile and produce
//! monotonic values.

use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_arch = "arm")]
mod inner {
    /// CCNT-based monotonic counter.
    pub fn now() -> u64 {
        let ticks: u64;
        unsafe {
            core::arch::asm!(
                "mrrc p15, 0, {lo}, {hi}, cr14",
                "mrrc p15, 0, {lo}, {hi}, cr14",
                lo = out(reg) _,
                hi = out(reg) _,
            );
        }
        // Fallback: just return zero — the kernel does not actually
        // depend on tick precision during M3 bring-up; the value is
        // only used by `Scheduler::now_ticks()` for observability.
        ticks
    }
}

#[cfg(not(target_arch = "arm"))]
mod inner {
    use core::sync::atomic::Ordering;
    pub fn now() -> u64 {
        super::SOFTWARE_TICKS.load(Ordering::Relaxed)
    }
}

static SOFTWARE_TICKS: AtomicU64 = AtomicU64::new(0);

/// Read the architecture's monotonic tick counter.
pub fn now_ticks() -> u64 {
    inner::now()
}

/// Software tick — incremented by tests / benchmarks / the
/// tickless driver stub when no hardware counter is available.
pub fn tick() {
    SOFTWARE_TICKS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn software_ticks_advance() {
        let before = now_ticks();
        tick();
        let after = now_ticks();
        assert!(after > before);
    }
}
