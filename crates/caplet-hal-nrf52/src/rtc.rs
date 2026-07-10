//! Low-frequency RTC for tick scheduling.
//!
//! The nRF52 RTC ticks at 32_768 Hz from the LFCLK oscillator and
//! provides a 24-bit counter plus four compare channels. We use it
//! for kernel time-keeping because it stays accurate in
//! System-On sleep modes.

use crate::raw::{self, nrf52832};
use caplet_hal::HalError;

/// RTC instance.
pub struct Rtc {
    base: *mut raw::RtcBlock,
    /// 32_768 for normal LFCLK
    clock_hz: u32,
}

unsafe impl Send for Rtc {}

impl Rtc {
    /// Take RTC0.
    pub unsafe fn rtc0() -> Self {
        Self {
            base: nrf52832::RTC0 as *mut raw::RtcBlock,
            clock_hz: 32_768,
        }
    }

    /// Start the RTC.
    pub fn start(&mut self) -> Result<(), HalError> {
        unsafe { raw::write32(self.base as usize, 1) }
        Ok(())
    }

    /// Stop the RTC.
    pub fn stop(&mut self) -> Result<(), HalError> {
        unsafe { raw::write32(self.base as usize + 0x004, 1) }
        Ok(())
    }

    /// Clear the counter to zero.
    pub fn clear(&mut self) -> Result<(), HalError> {
        unsafe { raw::write32(self.base as usize + 0x008, 1) }
        Ok(())
    }

    /// Read the current counter value.
    pub fn counter(&self) -> u32 {
        unsafe { raw::read32(self.base as usize + 0x504) }
    }

    /// Program the first compare register with a relative delay
    /// (in ticks) and enable the compare[0] interrupt.
    pub fn schedule_in(&mut self, delay_ticks: u32) -> Result<(), HalError> {
        let now = self.counter();
        unsafe {
            raw::write32(self.base as usize + 0x540, now.wrapping_add(delay_ticks));
            raw::set_bits(self.base as usize + 0x304, 1 << 16);
        }
        Ok(())
    }

    /// Acknowledge the compare[0] interrupt.
    pub fn ack_compare0(&mut self) {
        unsafe { raw::write32(self.base as usize + 0x140, 0) }
    }

    /// Convert milliseconds to RTC ticks.
    pub fn ms_to_ticks(&self, ms: u32) -> u32 {
        (ms * self.clock_hz) / 1_000
    }

    /// Convert microseconds to RTC ticks.
    pub fn us_to_ticks(&self, us: u32) -> u32 {
        // Note: with LFCLK = 32768 Hz, each tick is ~30.5 µs, so
        // we'll lose precision when scheduling sub-tick delays.
        (us * self.clock_hz) / 1_000_000
    }
}