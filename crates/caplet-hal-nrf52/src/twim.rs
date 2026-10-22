//! Blocking I2C master backed by the nRF52 TWIM peripheral.
//!
//! TWIM transfers data with EasyDMA. The driver waits for the documented
//! `LASTTX`/`LASTRX` and `STOPPED` events, checks `ERRORSRC`, and never falls
//! back to GPIO bit-banging.

use crate::raw::{self, nrf52832};
use alloc::vec::Vec;
use caplet_hal::i2c::{I2c, I2cAddrMode, I2cConfig, I2cSpeed};
use caplet_hal::HalError;

const TWIM_ENABLE: u32 = 6;
const ERROR_OVERRUN: u32 = 1 << 0;
const ERROR_ANACK: u32 = 1 << 1;
const ERROR_DNACK: u32 = 1 << 2;

/// TWIM instance.
pub struct Twim {
    base: *mut raw::TwimBlock,
    config: I2cConfig,
}

// SAFETY: The driver has exclusive ownership of the MMIO block and only uses
// volatile accesses.
unsafe impl Send for Twim {}

impl Twim {
    /// Take TWIM0.
    ///
    /// # Safety
    ///
    /// The caller must ensure TWIM0 is not also owned by another driver and
    /// that the selected pins are not used by another peripheral.
    pub unsafe fn twim0() -> Self {
        Self::new(nrf52832::TWIM0)
    }

    /// Take TWIM1.
    ///
    /// # Safety
    ///
    /// The caller must ensure TWIM1 is not also owned by another driver and
    /// that the selected pins are not used by another peripheral.
    pub unsafe fn twim1() -> Self {
        Self::new(nrf52832::TWIM1)
    }

    unsafe fn new(address: usize) -> Self {
        Self {
            base: address as *mut raw::TwimBlock,
            config: I2cConfig::default(),
        }
    }

    /// Select the SCL and SDA pins (P0 or P1 indices).
    pub fn set_pins(&mut self, scl: u8, sda: u8) -> Result<(), HalError> {
        if scl > 31 || sda > 31 || scl == sda {
            return Err(HalError::InvalidArgument);
        }
        unsafe {
            let block = &mut *self.base;
            block.psel_scl = u32::from(scl);
            block.psel_sda = u32::from(sda);
        }
        Ok(())
    }

    fn frequency_selector(speed: I2cSpeed) -> Result<u32, HalError> {
        match speed {
            I2cSpeed::Standard => Ok(0x0198_0000),
            I2cSpeed::Fast => Ok(0x0640_0000),
            I2cSpeed::FastPlus | I2cSpeed::HighSpeed => Err(HalError::NotSupported),
        }
    }

    fn validate_address(&self, address: u16) -> Result<u32, HalError> {
        if self.config.addr_mode != I2cAddrMode::Addr7Bit || address > 0x7F {
            Err(HalError::NotSupported)
        } else {
            Ok(u32::from(address))
        }
    }

    unsafe fn check_error(&self) -> Result<(), HalError> {
        let block = &mut *self.base;
        if block.events_error == 0 {
            return Ok(());
        }
        let source = block.errorsrc;
        // ERRORSRC is write-one-to-clear: write back the captured flags.
        core::ptr::write_volatile(core::ptr::addr_of_mut!(block.errorsrc), source);
        block.events_error = 0;
        if source & ERROR_OVERRUN != 0 {
            Err(HalError::Overrun)
        } else if source & (ERROR_ANACK | ERROR_DNACK) != 0 {
            Err(HalError::Nack)
        } else {
            Err(HalError::HardwareError)
        }
    }

    unsafe fn wait_event(&self, event: *mut u32) -> Result<(), HalError> {
        loop {
            self.check_error()?;
            if core::ptr::read_volatile(event) != 0 {
                core::ptr::write_volatile(event, 0);
                return Ok(());
            }
            core::hint::spin_loop();
        }
    }

    unsafe fn stop(&self) -> Result<(), HalError> {
        let block = &mut *self.base;
        block.tasks_stop = 1;
        self.wait_event(core::ptr::addr_of_mut!(block.events_stopped))
    }

    unsafe fn begin(&self, address: u32) {
        let block = &mut *self.base;
        block.address = address;
        block.events_stopped = 0;
        block.events_error = 0;
        block.events_lastrx = 0;
        block.events_lasttx = 0;
        block.errorsrc = u32::MAX;
    }

    unsafe fn send(&self, data: &[u8]) -> Result<(), HalError> {
        let length = u32::try_from(data.len()).map_err(|_| HalError::InvalidArgument)?;
        let block = &mut *self.base;
        block.txd_ptr = data.as_ptr() as u32;
        block.txd_maxcnt = length;
        block.tasks_starttx = 1;
        self.wait_event(core::ptr::addr_of_mut!(block.events_lasttx))
    }

    unsafe fn receive(&self, buffer: &mut [u8]) -> Result<(), HalError> {
        let length = u32::try_from(buffer.len()).map_err(|_| HalError::InvalidArgument)?;
        let block = &mut *self.base;
        block.rxd_ptr = buffer.as_mut_ptr() as u32;
        block.rxd_maxcnt = length;
        block.tasks_startrx = 1;
        self.wait_event(core::ptr::addr_of_mut!(block.events_lastrx))
    }

    unsafe fn probe_address(&self, address: u16) -> Result<(), HalError> {
        let address = self.validate_address(address)?;
        let mut byte = 0u8;
        self.begin(address);
        let result = self.receive(core::slice::from_mut(&mut byte));
        let stopped = self.stop();
        result.and(stopped)
    }
}

impl I2c for Twim {
    type Error = HalError;

    fn configure(&mut self, config: I2cConfig) -> Result<(), Self::Error> {
        if config.addr_mode != I2cAddrMode::Addr7Bit {
            return Err(HalError::NotSupported);
        }
        let frequency = Self::frequency_selector(config.speed)?;
        unsafe {
            let block = &mut *self.base;
            block.enable = 0;
            block.frequency = frequency;
            block.enable = TWIM_ENABLE;
        }
        self.config = config;
        Ok(())
    }

    fn write(&mut self, address: u16, data: &[u8]) -> Result<(), Self::Error> {
        let address = self.validate_address(address)?;
        if data.is_empty() {
            return Ok(());
        }
        unsafe {
            if (*self.base).enable != TWIM_ENABLE {
                return Err(HalError::NotReady);
            }
            self.begin(address);
            let result = self.send(data);
            let stopped = self.stop();
            result.and(stopped)
        }
    }

    fn read(&mut self, address: u16, buffer: &mut [u8]) -> Result<(), Self::Error> {
        let address = self.validate_address(address)?;
        if buffer.is_empty() {
            return Ok(());
        }
        unsafe {
            if (*self.base).enable != TWIM_ENABLE {
                return Err(HalError::NotReady);
            }
            self.begin(address);
            let result = self.receive(buffer);
            let stopped = self.stop();
            result.and(stopped)
        }
    }

    fn write_read(
        &mut self,
        address: u16,
        write: &[u8],
        read: &mut [u8],
    ) -> Result<(), Self::Error> {
        let address = self.validate_address(address)?;
        if write.is_empty() && read.is_empty() {
            return Ok(());
        }
        unsafe {
            if (*self.base).enable != TWIM_ENABLE {
                return Err(HalError::NotReady);
            }
            self.begin(address);
            if !write.is_empty() {
                self.send(write)?;
                // TWIM derives the R/W bit from the task (STARTRX versus
                // STARTTX), so the address register stays unchanged for the
                // repeated START.
            }
            let result = if read.is_empty() {
                Ok(())
            } else {
                self.receive(read)
            };
            let stopped = self.stop();
            result.and(stopped)
        }
    }

    fn scan(&self) -> Result<Vec<u8>, Self::Error> {
        let mut found = Vec::with_capacity(112);
        for address in 0x08..=0x77 {
            match unsafe { self.probe_address(address) } {
                Ok(()) => found.push(address as u8),
                Err(HalError::Nack) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_selectors_match_product_specification() {
        assert_eq!(
            Twim::frequency_selector(I2cSpeed::Standard),
            Ok(0x0198_0000)
        );
        assert_eq!(Twim::frequency_selector(I2cSpeed::Fast), Ok(0x0640_0000));
        assert_eq!(
            Twim::frequency_selector(I2cSpeed::FastPlus),
            Err(HalError::NotSupported)
        );
    }
}
