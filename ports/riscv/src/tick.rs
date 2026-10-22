//! Tick timer support for RISC-V (CLINT mtime).
//!
//! Provides a monotonic tick counter and tickless idle support.
//! The `TickTimer` wraps the CLINT mtime/mtimecmp registers.

use crate::clint::Clint;

/// CLINT timebase frequency on QEMU virt and Renode `riscv_virt`.
///
/// This is the CLINT clock — derived from the CPU timebase.
/// In a real system this would be configured by the board or devicetree.
pub const CLINT_HZ: u64 = 10_000_000;

/// Number of ticks per second.
pub const TICKS_PER_SEC: u64 = 1_000_000;

/// Convert a duration in milliseconds to ticks.
pub fn ms_to_ticks(ms: u64) -> u64 {
    (ms * TICKS_PER_SEC) / 1000
}

/// Convert ticks to milliseconds.
pub fn ticks_to_ms(ticks: u64) -> u64 {
    (ticks * 1000) / TICKS_PER_SEC
}

/// The tick timer used by the kernel.
///
/// Uses the CLINT machine timer (mtimecmp) for scheduling the next
/// deadline and reads mtime for the monotonic clock.
pub struct TickTimer;

impl TickTimer {
    /// Create a new tick timer.
    pub const fn new() -> Self {
        Self
    }

    /// Return the current tick count in microseconds.
    ///
    /// The tick is derived from CLINT mtime, which runs at CLINT_HZ.
    /// We normalize to 1 MHz ticks for consistency with the kernel's
    /// expected tick unit.
    pub fn now_ticks(&self) -> u64 {
        let mtime = Clint::mtime();
        // Normalize to 1 µs ticks.
        mtime / (CLINT_HZ / TICKS_PER_SEC)
    }

    /// Arm the timer to fire after `delta_ticks` microseconds.
    ///
    /// delta_ticks is in 1 MHz units (1 tick = 1 µs).
    pub fn set_deadline(&self, delta_ticks: u64) {
        // Convert µs ticks back to CLINT ticks.
        let clint_ticks_per_us = CLINT_HZ / TICKS_PER_SEC;
        let delta_clint = delta_ticks * clint_ticks_per_us;
        Clint::set_deadline(delta_clint);
    }

    /// Disable the timer (set mtimecmp = u64::MAX).
    pub fn disable(&self) {
        Clint::write_mtimecmp(0, u64::MAX);
    }

    /// Acknowledge the pending timer interrupt.
    ///
    /// Called from the trap handler. Must be called after each timer
    /// interrupt to re-arm the timer for the next deadline.
    pub fn ack(&self) {
        Clint::acknowledge_timer_irq();
    }

    /// Return the elapsed ticks since the last call.
    ///
    /// This is used by the kernel's tick accounting.
    pub fn elapsed_since(&self, last_ticks: u64) -> u64 {
        let now = self.now_ticks();
        now.saturating_sub(last_ticks)
    }
}

impl Default for TickTimer {
    fn default() -> Self {
        Self::new()
    }
}
