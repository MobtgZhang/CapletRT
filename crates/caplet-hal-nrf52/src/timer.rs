//! TIMER driver (16/32-bit counter).

use crate::raw::{self, nrf52832};
use caplet_hal::timer::{Timer as TimerTrait, TimerConfig, TimerRunMode};
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
    ///
    /// # Safety
    ///
    /// The caller must ensure TIMER0 is not already owned by another driver.
    pub unsafe fn timer0(clock_hz: u32) -> Self {
        Self {
            base: nrf52832::TIMER0 as *mut raw::TimBlock,
            clock_hz,
        }
    }

    fn period_ticks(&self, period_us: u32) -> u32 {
        let source_ticks = (u64::from(self.clock_hz) * u64::from(period_us.max(1))) / 1_000_000;
        let mut prescaler = 0u32;
        while prescaler < 15 && (source_ticks >> prescaler) > u64::from(u32::MAX) {
            prescaler += 1;
        }
        ((source_ticks >> prescaler).max(1)).min(u64::from(u32::MAX)) as u32
    }

    fn prescaler(&self, period_us: u32) -> u32 {
        let source_ticks = (u64::from(self.clock_hz) * u64::from(period_us.max(1))) / 1_000_000;
        let mut prescaler = 0u32;
        while prescaler < 15 && (source_ticks >> prescaler) > u64::from(u32::MAX) {
            prescaler += 1;
        }
        prescaler
    }
}

impl TimerTrait for Timer {
    type Error = HalError;

    fn configure(&mut self, config: TimerConfig) -> Result<(), Self::Error> {
        if self.clock_hz == 0 {
            return Err(HalError::InvalidArgument);
        }
        let prescaler = self.prescaler(config.period_us);
        let compare = self.period_ticks(config.period_us);
        unsafe {
            let block = &mut *self.base;
            block.tasks_stop = 1;
            block.tasks_clear = 1;
            block.mode = 0;
            block.bitmode = 3;
            block.prescaler = prescaler;
            block.cc[0] = compare;
            // SHORTS: COMPARE0_CLEAR (bit 0) always resets the counter;
            // COMPARE0_STOP (bit 1) additionally stops it for one-shot mode.
            let shorts = match config.mode {
                TimerRunMode::OneShot => 0b11,
                TimerRunMode::Periodic => 0b01,
            };
            block.shorts = shorts;
        }
        Ok(())
    }

    fn start(&mut self) -> Result<(), Self::Error> {
        unsafe { (*self.base).tasks_start = 1 }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        unsafe { (*self.base).tasks_stop = 1 }
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Self::Error> {
        unsafe { (*self.base).tasks_clear = 1 }
        Ok(())
    }

    fn count(&self) -> u32 {
        unsafe {
            let block = &mut *self.base;
            block.tasks_capture[0] = 1;
            block.cc[0]
        }
    }

    fn set_period(&mut self, period_us: u32) -> Result<(), Self::Error> {
        self.configure(TimerConfig {
            period_us,
            auto_reload: true,
            mode: TimerRunMode::Periodic,
        })
    }

    fn enable_interrupt(&mut self) -> Result<(), Self::Error> {
        unsafe { (*self.base).intenset = 1 << 16 }
        Ok(())
    }

    fn disable_interrupt(&mut self) -> Result<(), Self::Error> {
        unsafe { (*self.base).intenclr = 1 << 16 }
        Ok(())
    }

    fn clear_interrupt(&mut self) -> Result<(), Self::Error> {
        unsafe { (*self.base).events_compare[0] = 0 }
        Ok(())
    }

    fn is_interrupt_pending(&self) -> bool {
        unsafe { (*self.base).events_compare[0] != 0 }
    }

    fn set_run_mode(&mut self, mode: TimerRunMode) -> Result<(), Self::Error> {
        let shorts = match mode {
            TimerRunMode::OneShot => 0b11,
            TimerRunMode::Periodic => 0b01,
        };
        unsafe { (*self.base).shorts = shorts }
        Ok(())
    }

    fn set_deadline_ticks(&mut self, deadline_ticks: u32) -> Result<(), Self::Error> {
        let now = self.now_ticks();
        let delta = deadline_ticks.saturating_sub(now).max(1);
        unsafe {
            let block = &mut *self.base;
            block.cc[0] = delta;
            block.shorts = 0b11;
        }
        self.start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prescaler_keeps_compare_in_32_bit_range() {
        let timer = Timer {
            base: core::ptr::null_mut(),
            clock_hz: 16_000_000,
        };
        assert_eq!(timer.prescaler(1_000), 0);
        assert_eq!(timer.period_ticks(1_000), 16_000);
        assert_eq!(timer.prescaler(1_000_000), 0);
        assert_eq!(timer.period_ticks(1_000_000), 16_000_000);
        assert_eq!(timer.prescaler(300_000_000), 1);
        assert_eq!(timer.period_ticks(300_000_000), 2_400_000_000);
    }
}
