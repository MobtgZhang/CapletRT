//! Host-side regression test for the tickless-idle path.
//!
//! Verifies that [`tickless::record_wakeup`] properly increments
//! the wakeup counter, and that [`main_task::compute_idle_delta`]
//! returns sensible values for typical scheduler inputs.

#[cfg(test)]
mod tests {
    use caplet_kernel::main_task::compute_idle_delta;
    use caplet_kernel::tickless::{
        record_wakeup, reset_wakeup_count, IDLE_TICKS_HI, IDLE_TICKS_LO, WAKEUP_COUNT,
    };

    #[test]
    fn wakeup_counter_advances() {
        reset_wakeup_count();
        let initial = WAKEUP_COUNT.load(core::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            initial, 0,
            "WAKEUP_COUNT after reset should be 0, got {}",
            initial
        );
        record_wakeup(10);
        record_wakeup(20);
        let v = WAKEUP_COUNT.load(core::sync::atomic::Ordering::Relaxed);
        let lo = IDLE_TICKS_LO.load(core::sync::atomic::Ordering::Relaxed);
        let hi = IDLE_TICKS_HI.load(core::sync::atomic::Ordering::Relaxed);
        assert_eq!(v, 2);
        assert_eq!(hi, 0);
        assert_eq!(lo, 30);
    }

    #[test]
    fn idle_ticks_64bit_carry() {
        reset_wakeup_count();
        // Force a 32-bit wrap on the low half to exercise the carry path.
        record_wakeup(u32::MAX - 5);
        record_wakeup(10);
        let lo = IDLE_TICKS_LO.load(core::sync::atomic::Ordering::Relaxed);
        let hi = IDLE_TICKS_HI.load(core::sync::atomic::Ordering::Relaxed);
        assert_eq!(hi, 1, "high half should be incremented on carry");
        assert_eq!(lo, 4, "low half should wrap modulo 2^32");
    }

    #[test]
    fn idle_delta_handles_edge_cases() {
        // No deadline -> 1 tick (fallback).
        assert_eq!(compute_idle_delta(0, 100), 1);
        // Past deadline -> 1 tick.
        assert_eq!(compute_idle_delta(50, 100), 1);
        // Future deadline -> exact delta.
        assert_eq!(compute_idle_delta(150, 100), 50);
        // Equal deadline -> 1 tick (== treated as past).
        assert_eq!(compute_idle_delta(100, 100), 1);
    }
}
