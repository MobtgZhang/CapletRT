//! TIMER driver (16/32-bit counter).

use crate::raw::{self, nrf52832};
use caplet_hal::timer::{Timer as TimerTrait, TimerConfig};
use caplet_hal::HalError;

/// TIMER instance.
pub struct Timer {
    base: *mut raw::TimBlock,
    /// Timer clock in Hz (default 16 MHz).
    clock_hz: u32,
}

unsafe impl Send for Timer {}

impl Timer {
    /// Take TIMER0.
    pub unsafe fn timer0(clock_hz: u32) -> Self {
        Self {
            base: nrf52832::TIMER0 as *mut raw::TimBlock,
            clock_hz,
        }
    }
}

impl TimerTrait for Timer {
    type Error = HalError;

    fn configure(&mut self, config: TimerConfig) -> Result<(), Self::Error> {
        let target = self.clock_hz / 1_000_000 * config.period_us.max(1);
        let psc = (self.clock_hz / target / 65_536).max(1) - 1;
        let cc0 = target / (psc as u32 + 1);
        let base = self.base as usize;
        unsafe {
            // Stop the timer first.
            raw::write32(base + 0x004, 1);
            // Clear it.
            raw::write32(base + 0x00C, 1);
            // Mode: 0 = Timer, 1 = Counter.
            raw::write32(base + 0x408, 0);
            // Bit mode: 0 = 16 bit, 1 = 8 bit, 2 = 24 bit, 3 = 32 bit.
            raw::write32(base + 0x40C, 3);
            // Prescaler.
            raw::write32(base + 0x410, psc as u32);
            // Compare[0] = period.
            raw::write32(base + 0x540, cc0);
        }
        Ok(())
    }

    fn start(&mut self) -> Result<(), Self::Error> {
        unsafe { raw::write32(self.base as usize, 1) }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        unsafe { raw::write32(self.base as usize + 0x004, 1) }
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Self::Error> {
        unsafe { raw::write32(self.base as usize + 0x00C, 1) }
        Ok(())
    }

    fn count(&self) -> u32 {
        unsafe { raw::read32(self.base as usize + 0x504) }
    }

    fn set_period(&mut self, period_us: u32) -> Result<(), Self::Error> {
        self.configure(TimerConfig {
            period_us,
            auto_reload: true,
        })
    }

    fn enable_interrupt(&mut self) -> Result<(), Self::Error> {
        unsafe { raw::set_bits(self.base as usize + 0x304, 1 << 16) }
        Ok(())
    }

    fn disable_interrupt(&mut self) -> Result<(), Self::Error> {
        unsafe { raw::set_bits(self.base as usize + 0x308, 1 << 16) }
        Ok(())
    }

    fn clear_interrupt(&mut self) -> Result<(), Self::Error> {
        unsafe { raw::write32(self.base as usize + 0x140, 0) }
        Ok(())
    }

    fn is_interrupt_pending(&self) -> bool {
        unsafe { (raw::read32(self.base as usize + 0x140) & (1 << 16)) != 0 }
    }
}