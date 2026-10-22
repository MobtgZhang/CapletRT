//! General-purpose timer (TIM2..TIM5, TIM9..TIM11)

use crate::raw::{self, f446};
use caplet_hal::timer::{Timer, TimerConfig, TimerRunMode};
use caplet_hal::HalError;

/// Microseconds-based timer.
pub struct StmTimer {
    base: *mut raw::TimBlock,
    /// APB clock in Hz.
    clock_hz: u32,
}

unsafe impl Send for StmTimer {}

impl StmTimer {
    /// Take TIM2.
    ///
    /// # Safety
    /// The caller must ensure TIM2 is not owned elsewhere and that its APB
    /// clock is enabled (`clock_hz` must match the actual timer clock).
    pub unsafe fn tim2(clock_hz: u32) -> Self {
        Self {
            base: f446::TIM2 as *mut raw::TimBlock,
            clock_hz,
        }
    }

    /// Take TIM3.
    ///
    /// # Safety
    /// The caller must ensure TIM3 is not owned elsewhere and that its APB
    /// clock is enabled (`clock_hz` must match the actual timer clock).
    pub unsafe fn tim3(clock_hz: u32) -> Self {
        Self {
            base: f446::TIM3 as *mut raw::TimBlock,
            clock_hz,
        }
    }

    /// Set the prescaler (16-bit value, divides the clock by
    /// `psc + 1`).
    pub fn set_prescaler(&mut self, psc: u16) -> Result<(), HalError> {
        unsafe {
            raw::write32(self.base as usize + 0x28, psc as u32);
        }
        Ok(())
    }

    /// Set the auto-reload (period). When the counter reaches
    /// this value the next tick wraps to zero (and a UIF is
    /// emitted).
    pub fn set_auto_reload(&mut self, arr: u32) -> Result<(), HalError> {
        unsafe {
            raw::write32(self.base as usize + 0x2C, arr);
        }
        Ok(())
    }
}

impl Timer for StmTimer {
    type Error = HalError;

    fn configure(&mut self, config: TimerConfig) -> Result<(), Self::Error> {
        // Convert period (us) → target ticks at this clock.
        let target = self.clock_hz / 1_000_000 * config.period_us.max(1);
        let psc = (self.clock_hz / target / 65_536).max(1) - 1;
        let arr = target / (psc + 1);
        self.set_prescaler(psc as u16)?;
        self.set_auto_reload(arr)?;
        self.set_run_mode(config.mode)?;
        Ok(())
    }

    fn start(&mut self) -> Result<(), Self::Error> {
        unsafe {
            raw::set_bits(self.base as usize, 1 << 0);
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Self::Error> {
        unsafe {
            raw::clear_bits(self.base as usize, 1 << 0);
        }
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Self::Error> {
        unsafe {
            raw::write32(self.base as usize + 0x24, 0);
        }
        Ok(())
    }

    fn count(&self) -> u32 {
        unsafe { raw::read32(self.base as usize + 0x24) }
    }

    fn set_period(&mut self, period_us: u32) -> Result<(), Self::Error> {
        self.configure(TimerConfig::periodic(period_us))
    }

    fn enable_interrupt(&mut self) -> Result<(), Self::Error> {
        unsafe {
            raw::set_bits(self.base as usize + 0x0C, 1 << 0);
        }
        Ok(())
    }

    fn disable_interrupt(&mut self) -> Result<(), Self::Error> {
        unsafe {
            raw::clear_bits(self.base as usize + 0x0C, 1 << 0);
        }
        Ok(())
    }

    fn clear_interrupt(&mut self) -> Result<(), Self::Error> {
        unsafe {
            raw::clear_bits(self.base as usize + 0x10, 0x1);
        }
        Ok(())
    }

    fn is_interrupt_pending(&self) -> bool {
        unsafe { (raw::read32(self.base as usize + 0x10) & 0x1) != 0 }
    }

    fn set_run_mode(&mut self, mode: TimerRunMode) -> Result<(), Self::Error> {
        // STM32 TIMx CR1.OPM (bit 3): stop after the next update event.
        unsafe {
            match mode {
                TimerRunMode::OneShot => raw::set_bits(self.base as usize, 1 << 3),
                TimerRunMode::Periodic => raw::clear_bits(self.base as usize, 1 << 3),
            }
        }
        Ok(())
    }

    fn set_deadline_ticks(&mut self, deadline_ticks: u32) -> Result<(), Self::Error> {
        let now = self.now_ticks();
        let delta = deadline_ticks.saturating_sub(now).max(1);
        self.set_auto_reload(delta)?;
        self.set_run_mode(TimerRunMode::OneShot)?;
        self.start()
    }
}
