//! Blocking SPI master backed by the nRF52 SPIM peripheral.

use crate::raw::{self, nrf52832, nrf52840};
use caplet_hal::spi::{BitOrder, DataSize, Spi, SpiConfig, SpiMode};
use caplet_hal::HalError;

const SPIM_ENABLE: u32 = 7;
const PIN_DISCONNECTED: u32 = 0xFFFF_FFFF;

/// SPIM instance.
pub struct Spim {
    base: *mut raw::SpimBlock,
    config: SpiConfig,
}

// SAFETY: The driver has exclusive ownership of the MMIO block and only uses
// volatile accesses.
unsafe impl Send for Spim {}

impl Spim {
    /// Take SPIM0.
    ///
    /// # Safety
    ///
    /// The caller must ensure SPIM0 is not owned elsewhere and that its pins
    /// do not conflict with another peripheral.
    pub unsafe fn spim0() -> Self {
        Self::new(nrf52832::SPIM0)
    }

    /// Take SPIM1.
    ///
    /// # Safety
    ///
    /// The caller must ensure SPIM1 is not owned elsewhere and that its pins
    /// do not conflict with another peripheral.
    pub unsafe fn spim1() -> Self {
        Self::new(nrf52832::SPIM1)
    }

    /// Take SPIM2.
    ///
    /// # Safety
    ///
    /// The caller must ensure SPIM2 is not owned elsewhere and that its pins
    /// do not conflict with another peripheral.
    pub unsafe fn spim2() -> Self {
        Self::new(nrf52832::SPIM2)
    }

    /// Take the nRF52840-only SPIM3 instance.
    ///
    /// # Safety
    ///
    /// The caller must ensure the silicon is an nRF52840 (SPIM3 is absent on
    /// nRF52832), that SPIM3 is not owned elsewhere, and that its pins do not
    /// conflict with another peripheral.
    pub unsafe fn spim3_nrf52840() -> Self {
        Self::new(nrf52840::SPIM3)
    }

    unsafe fn new(address: usize) -> Self {
        Self {
            base: address as *mut raw::SpimBlock,
            config: SpiConfig::default(),
        }
    }

    /// Select SCK, MOSI, MISO and an optional CSN pin.
    pub fn set_pins(
        &mut self,
        sck: u8,
        mosi: u8,
        miso: u8,
        csn: Option<u8>,
    ) -> Result<(), HalError> {
        if sck > 31 || mosi > 31 || miso > 31 || csn.is_some_and(|pin| pin > 31) {
            return Err(HalError::InvalidArgument);
        }
        unsafe {
            let block = &mut *self.base;
            block.psel_sck = u32::from(sck);
            block.psel_mosi = u32::from(mosi);
            block.psel_miso = u32::from(miso);
            block.psel_csn = csn.map_or(PIN_DISCONNECTED, u32::from);
        }
        Ok(())
    }

    fn frequency_selector(frequency: u32) -> Result<u32, HalError> {
        match frequency {
            125_000 => Ok(0x0200_0000),
            250_000 => Ok(0x0400_0000),
            500_000 => Ok(0x0800_0000),
            1_000_000 => Ok(0x1000_0000),
            2_000_000 => Ok(0x2000_0000),
            4_000_000 => Ok(0x4000_0000),
            8_000_000 => Ok(0x8000_0000),
            _ => Err(HalError::NotSupported),
        }
    }

    fn mode_value(mode: SpiMode) -> u32 {
        match (mode.polarity, mode.phase) {
            (false, false) => 0,
            (false, true) => 1,
            (true, false) => 2,
            (true, true) => 3,
        }
    }

    unsafe fn wait_end(&self) -> Result<(), HalError> {
        loop {
            let block = &*self.base;
            if block.events_end != 0 {
                (*self.base).events_end = 0;
                return Ok(());
            }
            if block.events_stopped != 0 {
                (*self.base).events_stopped = 0;
                return Err(HalError::HardwareError);
            }
            core::hint::spin_loop();
        }
    }

    unsafe fn wait_stopped(&self) -> Result<(), HalError> {
        let block = &mut *self.base;
        block.tasks_stop = 1;
        loop {
            if block.events_stopped != 0 {
                block.events_stopped = 0;
                return Ok(());
            }
            core::hint::spin_loop();
        }
    }

    unsafe fn transfer_inner(&self, read: &mut [u8], write: &[u8]) -> Result<(), HalError> {
        if read.is_empty() && write.is_empty() {
            return Ok(());
        }
        if !read.is_empty() && !write.is_empty() && read.len() != write.len() {
            return Err(HalError::InvalidArgument);
        }
        if (*self.base).enable != SPIM_ENABLE {
            return Err(HalError::NotReady);
        }

        let block = &mut *self.base;
        block.events_end = 0;
        block.events_endrx = 0;
        block.events_endtx = 0;
        block.events_stopped = 0;
        block.rxd_ptr = if read.is_empty() {
            0
        } else {
            read.as_mut_ptr() as u32
        };
        block.rxd_maxcnt = u32::try_from(read.len()).map_err(|_| HalError::InvalidArgument)?;
        block.txd_ptr = if write.is_empty() {
            0
        } else {
            write.as_ptr() as u32
        };
        block.txd_maxcnt = u32::try_from(write.len()).map_err(|_| HalError::InvalidArgument)?;
        block.tasks_start = 1;
        self.wait_end()?;
        self.wait_stopped()
    }
}

impl Spi for Spim {
    type Error = HalError;

    fn configure(&mut self, config: SpiConfig) -> Result<(), Self::Error> {
        if config.data_size != DataSize::Bits8 || config.bit_order != BitOrder::MsbFirst {
            return Err(HalError::NotSupported);
        }
        let frequency = Self::frequency_selector(config.frequency)?;
        let mode = Self::mode_value(config.mode);
        unsafe {
            let block = &mut *self.base;
            block.enable = 0;
            block.config = mode;
            block.frequency = frequency;
            block.orc = 0xFF;
            block.enable = SPIM_ENABLE;
        }
        self.config = config;
        Ok(())
    }

    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
        unsafe { self.transfer_inner(read, write) }
    }

    fn write(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        unsafe { self.transfer_inner(&mut [], data) }
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<(), Self::Error> {
        unsafe { self.transfer_inner(buffer, &[]) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_selectors_match_product_specification() {
        assert_eq!(Spim::frequency_selector(125_000), Ok(0x0200_0000));
        assert_eq!(Spim::frequency_selector(1_000_000), Ok(0x1000_0000));
        assert_eq!(Spim::frequency_selector(8_000_000), Ok(0x8000_0000));
        assert_eq!(
            Spim::frequency_selector(3_000_000),
            Err(HalError::NotSupported)
        );
    }

    #[test]
    fn four_spi_modes_map_to_config_register_values() {
        assert_eq!(Spim::mode_value(SpiMode::mode0()), 0);
        assert_eq!(Spim::mode_value(SpiMode::mode1()), 1);
        assert_eq!(Spim::mode_value(SpiMode::mode2()), 2);
        assert_eq!(Spim::mode_value(SpiMode::mode3()), 3);
    }
}
