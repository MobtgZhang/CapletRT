//! Monotonic tick counter for Cortex-A.
//
//! docv1.md §6.4 specifies a SP804 dual timer.  Both QEMU's
//! `vexpress-a9` machine and the Renode platform used by this
//! repository expose an SP804 at `0x1001_1000`, so Timer 1 is used as
//! a free-running 32-bit down-counter.  Each wrap is folded into a
//! 64-bit software accumulator, giving a monotonic microsecond-ish
//! tick counter (the SP804 runs at 1 MHz on both models).
//
//! On hosts (no ARM target) we fall back to a software counter
//! incremented in `tick()` so tests still compile and produce
//! monotonic values.

use core::sync::atomic::AtomicU32;

static SOFTWARE_TICKS: AtomicU32 = AtomicU32::new(0);

#[cfg(target_arch = "arm")]
mod inner {
    use super::*;
    use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    const TIMER1_BASE: usize = 0x1001_1000;
    const TIMER1_LOAD: usize = TIMER1_BASE;
    const TIMER1_VALUE: usize = TIMER1_BASE + 0x04;
    const TIMER1_CONTROL: usize = TIMER1_BASE + 0x08;

    static INITIALISED: AtomicBool = AtomicBool::new(false);
    static LAST_VALUE: AtomicU32 = AtomicU32::new(0);
    static ACCUMULATED: AtomicU64 = AtomicU64::new(0);

    fn init_once() {
        if INITIALISED.swap(true, Ordering::AcqRel) {
            return;
        }
        unsafe {
            // 32-bit, periodic, enabled, no interrupt.  The timer
            // counts down at 1 MHz on vexpress-a9.
            core::ptr::write_volatile(TIMER1_LOAD as *mut u32, u32::MAX);
            core::ptr::write_volatile(TIMER1_CONTROL as *mut u32, 0x82);
            LAST_VALUE.store(
                core::ptr::read_volatile(TIMER1_VALUE as *const u32),
                Ordering::Release,
            );
        }
    }

    pub fn now() -> u64 {
        init_once();
        let current = unsafe { core::ptr::read_volatile(TIMER1_VALUE as *const u32) };
        let previous = LAST_VALUE.swap(current, Ordering::AcqRel);
        // The counter counts down, so a wrap shows up as
        // `previous < current`.
        let delta = previous.wrapping_sub(current);
        ACCUMULATED.fetch_add(delta as u64, Ordering::Relaxed) + delta as u64
    }
}

#[cfg(not(target_arch = "arm"))]
mod inner {
    use core::sync::atomic::Ordering;
    pub fn now() -> u64 {
        super::SOFTWARE_TICKS.load(Ordering::Relaxed) as u64
    }
}

/// Read the architecture's monotonic tick counter.
pub fn now_ticks() -> u64 {
    inner::now()
}

/// Software tick — incremented by tests / benchmarks / the
/// tickless driver stub when no hardware counter is available.
pub fn tick() {
    SOFTWARE_TICKS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
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
