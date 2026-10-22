//! SPI master for the STM32F4 SPI peripheral.
//!
//! The driver implements polled full-duplex transfers. It supports 8- and
//! 16-bit frames, both bit orders, all four SPI modes and the full SPI1..SPI6
//! instance range. Chip-select is intentionally not owned by this driver;
//! boards either use a GPIO wrapped in [`caplet_hal::spi::cs::ActiveLow`] or
//! keep the NSS pin in software-controlled mode.

use crate::raw::{self, f446};
use caplet_hal::spi::{BitOrder, DataSize, Spi, SpiConfig};
use caplet_hal::HalError;

const CR1_CPHA: u32 = 1 << 0;
const CR1_CPOL: u32 = 1 << 1;
const CR1_MSTR: u32 = 1 << 2;
const CR1_SPE: u32 = 1 << 6;
const CR1_LSBFIRST: u32 = 1 << 7;
const CR1_SSI: u32 = 1 << 8;
const CR1_SSM: u32 = 1 << 9;
const CR1_DFF: u32 = 1 << 11;

const SR_RXNE: u32 = 1 << 0;
const SR_TXE: u32 = 1 << 1;
const SR_CRCERR: u32 = 1 << 4;
const SR_MODF: u32 = 1 << 5;
const SR_OVR: u32 = 1 << 6;
const SR_BSY: u32 = 1 << 7;

const CR1: usize = 0x00;
const CR2: usize = 0x04;
const SR: usize = 0x08;
const DR: usize = 0x0C;

/// STM32F4 polled SPI master.
pub struct SpiMaster {
    base: *mut raw::SpiBlock,
    clock_hz: u32,
    config: SpiConfig,
}

// SAFETY: The driver owns one MMIO peripheral and all accesses are volatile.
unsafe impl Send for SpiMaster {}

impl SpiMaster {
    /// Take SPI1.
    ///
    /// # Safety
    /// The caller must ensure SPI1 is not owned elsewhere and that
    /// `clock_hz` is the APB2 clock feeding the peripheral.
    pub unsafe fn spi1(clock_hz: u32) -> Self {
        Self::new(f446::SPI1, clock_hz)
    }

    /// Take SPI2.
    ///
    /// # Safety
    /// The caller must ensure SPI2 is not owned elsewhere and that
    /// `clock_hz` is the APB1 clock feeding the peripheral.
    pub unsafe fn spi2(clock_hz: u32) -> Self {
        Self::new(f446::SPI2, clock_hz)
    }

    /// Take SPI3.
    ///
    /// # Safety
    /// The caller must ensure SPI3 is not owned elsewhere and that
    /// `clock_hz` is the APB1 clock feeding the peripheral.
    pub unsafe fn spi3(clock_hz: u32) -> Self {
        Self::new(f446::SPI3, clock_hz)
    }

    /// Take SPI4.
    ///
    /// # Safety
    /// The caller must ensure SPI4 is not owned elsewhere and that
    /// `clock_hz` is the APB2 clock feeding the peripheral.
    pub unsafe fn spi4(clock_hz: u32) -> Self {
        Self::new(f446::SPI4, clock_hz)
    }

    /// Take SPI5.
    ///
    /// # Safety
    /// The caller must ensure SPI5 is not owned elsewhere and that
    /// `clock_hz` is the APB2 clock feeding the peripheral.
    pub unsafe fn spi5(clock_hz: u32) -> Self {
        Self::new(f446::SPI5, clock_hz)
    }

    /// Take SPI6.
    ///
    /// # Safety
    /// The caller must ensure SPI6 is not owned elsewhere and that
    /// `clock_hz` is the APB2 clock feeding the peripheral.
    pub unsafe fn spi6(clock_hz: u32) -> Self {
        Self::new(f446::SPI6, clock_hz)
    }

    unsafe fn new(address: usize, clock_hz: u32) -> Self {
        Self {
            base: address as *mut raw::SpiBlock,
            clock_hz,
            config: SpiConfig::default(),
        }
    }

    fn base(&self) -> usize {
        self.base as usize
    }

    fn prescaler(&self, frequency: u32) -> Result<u32, HalError> {
        if frequency == 0 || frequency > self.clock_hz / 2 {
            return Err(HalError::InvalidArgument);
        }
        for br in 0..=7 {
            let divisor = 1u32 << (br + 1);
            if self.clock_hz / divisor <= frequency {
                return Ok(br);
            }
        }
        Err(HalError::InvalidArgument)
    }

    unsafe fn check_status(&self) -> Result<(), HalError> {
        let sr = raw::read32(self.base() + SR);
        if sr & (SR_MODF | SR_OVR | SR_CRCERR) != 0 {
            return Err(HalError::HardwareError);
        }
        Ok(())
    }

    unsafe fn wait_status(&self, mask: u32, set: bool) -> Result<(), HalError> {
        let mut budget = self.clock_hz.max(1_000_000);
        loop {
            self.check_status()?;
            let present = raw::read32(self.base() + SR) & mask != 0;
            if present == set {
                return Ok(());
            }
            if budget == 0 {
                return Err(HalError::Timeout);
            }
            budget -= 1;
            core::hint::spin_loop();
        }
    }

    unsafe fn wait_idle(&self) -> Result<(), HalError> {
        self.wait_status(SR_BSY, false)
    }

    unsafe fn send_word(&self, word: u16) -> Result<(), HalError> {
        self.wait_status(SR_TXE, true)?;
        raw::write32(self.base() + DR, u32::from(word));
        Ok(())
    }

    unsafe fn receive_word(&self) -> Result<u16, HalError> {
        self.wait_status(SR_RXNE, true)?;
        Ok(raw::read32(self.base() + DR) as u16)
    }

    unsafe fn transfer_8(&self, read: &mut [u8], write: &[u8]) -> Result<(), HalError> {
        let count = read.len().max(write.len());
        for index in 0..count {
            let tx = write.get(index).copied().unwrap_or(0xFF);
            self.send_word(u16::from(tx))?;
            let rx = self.receive_word()? as u8;
            if let Some(slot) = read.get_mut(index) {
                *slot = rx;
            }
        }
        self.wait_idle()
    }

    unsafe fn transfer_16(&self, read: &mut [u8], write: &[u8]) -> Result<(), HalError> {
        if read.len() & 1 != 0 || write.len() & 1 != 0 {
            return Err(HalError::InvalidArgument);
        }
        let count = read.len().max(write.len()) / 2;
        for index in 0..count {
            let tx = if write.len() >= (index + 1) * 2 {
                u16::from_le_bytes([write[index * 2], write[index * 2 + 1]])
            } else {
                0xFFFF
            };
            self.send_word(tx)?;
            let rx = self.receive_word()?;
            if read.len() >= (index + 1) * 2 {
                let bytes = rx.to_le_bytes();
                read[index * 2] = bytes[0];
                read[index * 2 + 1] = bytes[1];
            }
        }
        self.wait_idle()
    }
}

impl Spi for SpiMaster {
    type Error = HalError;

    fn configure(&mut self, config: SpiConfig) -> Result<(), Self::Error> {
        let br = self.prescaler(config.frequency)?;
        let mut cr1 = CR1_MSTR | CR1_SSM | CR1_SSI | (br << 3);
        if config.mode.polarity {
            cr1 |= CR1_CPOL;
        }
        if config.mode.phase {
            cr1 |= CR1_CPHA;
        }
        if config.data_size == DataSize::Bits16 {
            cr1 |= CR1_DFF;
        }
        if config.bit_order == BitOrder::LsbFirst {
            cr1 |= CR1_LSBFIRST;
        }

        unsafe {
            raw::clear_bits(self.base() + CR1, CR1_SPE);
            self.wait_idle()?;
            raw::write32(self.base() + CR2, 0);
            raw::write32(self.base() + CR1, cr1);
            raw::set_bits(self.base() + CR1, CR1_SPE);
        }
        self.config = config;
        Ok(())
    }

    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
        unsafe {
            self.check_status()?;
            match self.config.data_size {
                DataSize::Bits8 => self.transfer_8(read, write),
                DataSize::Bits16 => self.transfer_16(read, write),
            }
        }
    }

    fn write(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        unsafe {
            self.check_status()?;
            match self.config.data_size {
                DataSize::Bits8 => self.transfer_8(&mut [], data),
                DataSize::Bits16 => self.transfer_16(&mut [], data),
            }
        }
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<(), Self::Error> {
        unsafe {
            self.check_status()?;
            match self.config.data_size {
                DataSize::Bits8 => self.transfer_8(buffer, &[]),
                DataSize::Bits16 => self.transfer_16(buffer, &[]),
            }
        }
    }
}

/// Re-exported helper for boards that need the physical SPI mode without
/// importing `caplet_hal` directly.
pub use caplet_hal::spi::SpiMode;

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn register_offsets_match_rm0390() {
        assert_eq!(offset_of!(raw::SpiBlock, cr1), 0x00);
        assert_eq!(offset_of!(raw::SpiBlock, cr2), 0x04);
        assert_eq!(offset_of!(raw::SpiBlock, sr), 0x08);
        assert_eq!(offset_of!(raw::SpiBlock, dr), 0x0C);
        assert_eq!(offset_of!(raw::SpiBlock, i2scfgr), 0x1C);
        assert_eq!(offset_of!(raw::SpiBlock, i2spr), 0x20);
        assert_eq!(size_of::<raw::SpiBlock>(), 0x24);
    }

    #[test]
    fn prescaler_selects_closest_not_exceeding_clock() {
        let spi = SpiMaster {
            base: core::ptr::null_mut(),
            clock_hz: 84_000_000,
            config: SpiConfig::default(),
        };
        assert_eq!(spi.prescaler(42_000_000).unwrap(), 0);
        assert_eq!(spi.prescaler(21_000_000).unwrap(), 1);
        assert_eq!(spi.prescaler(13_125_000).unwrap(), 2);
        assert_eq!(spi.prescaler(1_000_000).unwrap(), 6);
        assert_eq!(spi.prescaler(84_000_000), Err(HalError::InvalidArgument));
        assert_eq!(spi.prescaler(0), Err(HalError::InvalidArgument));
    }
}
