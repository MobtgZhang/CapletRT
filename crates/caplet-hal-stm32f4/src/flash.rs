//! FLASH register access (for unlock + latency setting).

use crate::raw::{self, f446};
use caplet_hal::HalError;

/// Flash driver.
pub struct Flash {
    base: *mut raw::FlashBlock,
}

unsafe impl Send for Flash {}

impl Flash {
    /// Take the singleton.
    ///
    /// # Safety
    /// The caller must ensure this is the only live owner of the FLASH
    /// controller and that the flash latency change is safe for the current
    /// core clock and supply voltage.
    pub unsafe fn take() -> Self {
        Self {
            base: f446::FLASH as *mut raw::FlashBlock,
        }
    }

    /// Set the flash wait-state latency for a given CPU clock
    /// range.
    ///
    /// `wait_states` is the value written to the LATENCY bit-field
    /// of the ACR register (RM0390 §3.3.1). Pass 0..=7 depending
    /// on your actual VDD + clock.
    pub fn set_latency(&mut self, wait_states: u8) -> Result<(), HalError> {
        if wait_states > 15 {
            return Err(HalError::InvalidArgument);
        }
        unsafe {
            let acr = raw::read32(self.base as usize);
            let acr = (acr & !0xF) | (wait_states as u32 & 0xF);
            raw::write32(self.base as usize, acr);
        }
        Ok(())
    }

    /// Unlock the flash for programming / erasing. Must be called
    /// before any flash write/erase.
    pub fn unlock(&mut self) -> Result<(), HalError> {
        unsafe {
            raw::write32(self.base as usize + 0x04, 0x4567_0123);
            raw::write32(self.base as usize + 0x04, 0xCDEF_89AB);
        }
        Ok(())
    }

    /// Re-lock the flash.
    pub fn lock(&mut self) -> Result<(), HalError> {
        unsafe {
            raw::set_bits(self.base as usize + 0x10, 1 << 31);
        }
        Ok(())
    }
}
