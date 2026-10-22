//! CapletRT Timer subsystem
//!
//! Re-exports the [`wheel::TimerWheel`] algorithm and provides a
//! variable-reload SysTick helper for tickless idle.

#![no_std]
#![deny(missing_docs)]

extern crate alloc;

pub mod wheel;

pub use wheel::{TimerError, TimerId, TimerWheel, Token, BUCKETS, SLOTS};

/// Compute the next SysTick reload value so that the next
/// deadline is met *without* round-tripping through 1 ms ticks.
///
/// `now` and `deadline_ticks` are in milliseconds (or any
/// consistent unit). `ticks_per_ms` is the SysTick divider: at
/// 16 MHz with a `/1` divider, this is 16_000. The function
/// returns a *SysTick-load* value (also in cycles).
pub fn next_systick_reload(now_ms: u64, deadline_ms: u64, ticks_per_ms: u32) -> u32 {
    let delta_ms = deadline_ms.saturating_sub(now_ms);
    (delta_ms as u128 * ticks_per_ms as u128).min(u32::MAX as u128) as u32
}

/// Translate a tickless-deadline hint into a 24-bit reload value
/// for the SysTick (default Hardware). Suitable for ARM Cortex-M
/// SysTick (which is 24-bit wide).
pub fn systick_24bit_reload(delta: u32) -> u32 {
    (delta & 0x00FF_FFFF).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_systick_reload_returns_positive() {
        // From now=100 ms to deadline=105 ms -> 5 ms at 1000 ticks/ms = 5000.
        let r = next_systick_reload(100, 105, 1_000);
        assert_eq!(r, 5_000);
    }

    #[test]
    fn next_systick_reload_clamps() {
        let r = next_systick_reload(0, u64::MAX, 1_000);
        assert_eq!(r, u32::MAX);
    }

    #[test]
    fn next_systick_reload_saturates_in_the_past() {
        // Past deadline: 0 delta, but we still need a positive
        // reload, so the function returns 0 (caller should
        // re-arm).
        let r = next_systick_reload(50, 30, 1_000);
        assert_eq!(r, 0);
    }

    #[test]
    fn systick_24bit_reload_truncates() {
        assert_eq!(systick_24bit_reload(0xFFFF_FFFF), 0x00FF_FFFF);
        assert_eq!(systick_24bit_reload(0), 1);
    }
}
