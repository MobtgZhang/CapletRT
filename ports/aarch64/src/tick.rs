//! AArch64 generic timer helpers.

/// Read `CNTPCT_EL0` (the physical counter).
#[cfg(target_arch = "aarch64")]
pub fn now_ticks() -> u64 {
    let value: u64;
    // SAFETY: reading the physical counter is always permitted at EL1.
    unsafe { core::arch::asm!("mrs {0}, cntpct_el0", out(reg) value, options(nomem, nostack)) };
    value
}

/// Host stub.
#[cfg(not(target_arch = "aarch64"))]
pub fn now_ticks() -> u64 {
    0
}

/// Read `CNTFRQ_EL0`.
#[cfg(target_arch = "aarch64")]
pub fn frequency() -> u64 {
    let value: u64;
    // SAFETY: CNTFRQ_EL0 is readable at EL1.
    unsafe { core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) value, options(nomem, nostack)) };
    value
}

/// Host stub.
#[cfg(not(target_arch = "aarch64"))]
pub fn frequency() -> u64 {
    0
}

/// Convenience wrapper matching the other ports' API.
pub struct TickTimer;

impl TickTimer {
    /// Create a tick timer.
    pub const fn new() -> Self {
        Self
    }

    /// Current counter value.
    pub fn now_ticks(&self) -> u64 {
        now_ticks()
    }

    /// Counter frequency in Hz.
    pub fn frequency(&self) -> u64 {
        frequency()
    }
}

impl Default for TickTimer {
    fn default() -> Self {
        Self::new()
    }
}

/// Program the EL1 physical timer for a one-shot interrupt.
///
/// `ticks` is relative to the current `CNTPCT_EL0` value.  Interrupt
/// delivery must be enabled separately through the GIC and PSTATE.I.
#[cfg(target_arch = "aarch64")]
pub fn set_one_shot(ticks: u64) {
    // SAFETY: CNTP_TVAL_EL0/CNTP_CTL_EL0 are accessible at EL1.
    unsafe {
        core::arch::asm!(
            "msr cntp_tval_el0, {ticks}",
            "isb",
            "mov {tmp}, #1",
            "msr cntp_ctl_el0, {tmp}",
            "isb",
            ticks = in(reg) ticks,
            tmp = out(reg) _,
            options(nostack),
        );
    }
}

/// Host stub for [`set_one_shot`].
#[cfg(not(target_arch = "aarch64"))]
pub fn set_one_shot(_ticks: u64) {}

/// Program a periodic timer at `hz`.
#[cfg(target_arch = "aarch64")]
pub fn start_periodic(hz: u64) {
    let freq = frequency();
    let period = if hz == 0 { freq } else { freq / hz };
    set_one_shot(period.max(1));
}

/// Host stub for [`start_periodic`].
#[cfg(not(target_arch = "aarch64"))]
pub fn start_periodic(_hz: u64) {}

/// Stop the EL1 physical timer.
#[cfg(target_arch = "aarch64")]
pub fn stop() {
    // SAFETY: CNTP_CTL_EL0 is accessible at EL1.
    unsafe {
        core::arch::asm!(
            "msr cntp_ctl_el0, {value}",
            "isb",
            value = in(reg) 0u64,
            options(nostack),
        );
    }
}

/// Host stub for [`stop`].
#[cfg(not(target_arch = "aarch64"))]
pub fn stop() {}
