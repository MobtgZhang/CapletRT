//! Blocking I2C master for the STM32F4 I2C peripheral.
//!
//! The driver implements the full byte-level state machine from RM0390:
//! START/repeated-START, address phases for 7- and 10-bit targets, ACK/NACK,
//! data transfers and STOP. It deliberately does not advertise DMA support;
//! the HAL DMA methods return [`HalError::NotSupported`] instead of silently
//! pretending an interrupt-driven transfer completed.

use crate::raw::{self, f446};
use alloc::vec::Vec;
use caplet_hal::i2c::{I2c, I2cAddrMode, I2cConfig, I2cSpeed};
use caplet_hal::HalError;

const CR1_PE: u32 = 1 << 0;
const CR1_START: u32 = 1 << 8;
const CR1_STOP: u32 = 1 << 9;
const CR1_ACK: u32 = 1 << 10;

const SR1_SB: u32 = 1 << 0;
const SR1_ADDR: u32 = 1 << 1;
const SR1_BTF: u32 = 1 << 2;
const SR1_ADD10: u32 = 1 << 3;
const SR1_RXNE: u32 = 1 << 6;
const SR1_TXE: u32 = 1 << 7;
const SR1_BERR: u32 = 1 << 8;
const SR1_ARLO: u32 = 1 << 9;
const SR1_AF: u32 = 1 << 10;
const SR1_OVR: u32 = 1 << 11;
const SR1_TIMEOUT: u32 = 1 << 14;

const SR2_BUSY: u32 = 1 << 1;

const CCR_FS: u32 = 1 << 15;

const CR1: usize = 0x00;
const CR2: usize = 0x04;
const DR: usize = 0x10;
const SR1: usize = 0x14;
const SR2: usize = 0x18;
const CCR: usize = 0x1C;
const TRISE: usize = 0x20;
const FLTR: usize = 0x24;

/// STM32F4 I2C master.
pub struct I2cMaster {
    base: *mut raw::I2cBlock,
    clock_hz: u32,
    config: I2cConfig,
}

// SAFETY: The driver owns one MMIO peripheral and all accesses are volatile.
unsafe impl Send for I2cMaster {}

impl I2cMaster {
    /// Take I2C1.
    ///
    /// # Safety
    /// The caller must ensure I2C1 is not owned elsewhere and that
    /// `clock_hz` is the APB1 clock feeding the peripheral.
    pub unsafe fn i2c1(clock_hz: u32) -> Self {
        Self::new(f446::I2C1, clock_hz)
    }

    /// Take I2C2.
    ///
    /// # Safety
    /// The caller must ensure I2C2 is not owned elsewhere and that
    /// `clock_hz` is the APB1 clock feeding the peripheral.
    pub unsafe fn i2c2(clock_hz: u32) -> Self {
        Self::new(f446::I2C2, clock_hz)
    }

    /// Take I2C3.
    ///
    /// # Safety
    /// The caller must ensure I2C3 is not owned elsewhere and that
    /// `clock_hz` is the APB1 clock feeding the peripheral.
    pub unsafe fn i2c3(clock_hz: u32) -> Self {
        Self::new(f446::I2C3, clock_hz)
    }

    unsafe fn new(address: usize, clock_hz: u32) -> Self {
        Self {
            base: address as *mut raw::I2cBlock,
            clock_hz,
            config: I2cConfig::default(),
        }
    }

    fn base(&self) -> usize {
        self.base as usize
    }

    fn require_enabled(&self) -> Result<(), HalError> {
        if unsafe { raw::read32(self.base() + CR1) } & CR1_PE == 0 {
            Err(HalError::NotReady)
        } else {
            Ok(())
        }
    }

    fn timeout(&self) -> SpinTimeout {
        SpinTimeout::new(self.config.timeout_ms, self.clock_hz)
    }

    unsafe fn clear_error_flags(&self) {
        raw::clear_bits(
            self.base() + SR1,
            SR1_BERR | SR1_ARLO | SR1_AF | SR1_OVR | SR1_TIMEOUT,
        );
    }

    unsafe fn check_status(&self) -> Result<(), HalError> {
        let sr1 = raw::read32(self.base() + SR1);
        if sr1 & SR1_AF != 0 {
            raw::clear_bits(self.base() + SR1, SR1_AF);
            return Err(HalError::Nack);
        }
        if sr1 & SR1_ARLO != 0 {
            raw::clear_bits(self.base() + SR1, SR1_ARLO);
            return Err(HalError::ArbitrationLost);
        }
        if sr1 & SR1_TIMEOUT != 0 {
            raw::clear_bits(self.base() + SR1, SR1_TIMEOUT);
            return Err(HalError::Timeout);
        }
        if sr1 & (SR1_BERR | SR1_OVR) != 0 {
            raw::clear_bits(self.base() + SR1, SR1_BERR | SR1_OVR);
            return Err(HalError::HardwareError);
        }
        Ok(())
    }

    unsafe fn wait_sr1(&self, mask: u32, timeout: &mut SpinTimeout) -> Result<(), HalError> {
        loop {
            self.check_status()?;
            if raw::read32(self.base() + SR1) & mask != 0 {
                return Ok(());
            }
            timeout.tick()?;
            core::hint::spin_loop();
        }
    }

    unsafe fn wait_bus_free(&self, timeout: &mut SpinTimeout) -> Result<(), HalError> {
        loop {
            self.check_status()?;
            if raw::read32(self.base() + SR2) & SR2_BUSY == 0 {
                return Ok(());
            }
            if timeout.tick().is_err() {
                return Err(HalError::BusBusy);
            }
            core::hint::spin_loop();
        }
    }

    unsafe fn generate_start(&self, timeout: &mut SpinTimeout) -> Result<(), HalError> {
        let cr1 = raw::read32(self.base() + CR1);
        raw::write32(self.base() + CR1, cr1 | CR1_START);
        self.wait_sr1(SR1_SB, timeout)
    }

    unsafe fn start_condition(&self, timeout: &mut SpinTimeout) -> Result<(), HalError> {
        self.wait_bus_free(timeout)?;
        self.generate_start(timeout)
    }

    unsafe fn stop_condition(&self, timeout: &mut SpinTimeout) -> Result<(), HalError> {
        let cr1 = raw::read32(self.base() + CR1);
        raw::write32(self.base() + CR1, cr1 | CR1_STOP);

        loop {
            self.check_status()?;
            if raw::read32(self.base() + SR2) & SR2_BUSY == 0 {
                let _ = raw::read32(self.base() + SR1);
                let _ = raw::read32(self.base() + SR2);
                return Ok(());
            }
            timeout.tick()?;
            core::hint::spin_loop();
        }
    }

    unsafe fn abort(&self) {
        let cr1 = raw::read32(self.base() + CR1);
        raw::write32(self.base() + CR1, cr1 | CR1_STOP);
        self.clear_error_flags();
        let _ = raw::read32(self.base() + SR1);
        let _ = raw::read32(self.base() + SR2);
    }

    unsafe fn clear_addr_flag(&self) {
        let _ = raw::read32(self.base() + SR1);
        let _ = raw::read32(self.base() + SR2);
    }

    unsafe fn send_address(
        &self,
        address: u16,
        read: bool,
        timeout: &mut SpinTimeout,
    ) -> Result<(), HalError> {
        match self.config.addr_mode {
            I2cAddrMode::Addr7Bit => {
                if address > 0x7F {
                    return Err(HalError::InvalidArgument);
                }
                let byte = ((address as u32) << 1) | u32::from(read);
                raw::write32(self.base() + DR, byte);
                self.wait_sr1(SR1_ADDR, timeout)?;
                self.clear_addr_flag();
            }
            I2cAddrMode::Addr10Bit => {
                if address > 0x3FF {
                    return Err(HalError::InvalidArgument);
                }
                let header = 0xF0 | (((address >> 7) as u32) & 0x06);
                raw::write32(self.base() + DR, header);
                self.wait_sr1(SR1_ADD10, timeout)?;

                if read {
                    self.generate_start(timeout)?;
                    raw::write32(self.base() + DR, header | 1);
                } else {
                    raw::write32(self.base() + DR, u32::from(address & 0xFF));
                }
                self.wait_sr1(SR1_ADDR, timeout)?;
                self.clear_addr_flag();
            }
        }
        Ok(())
    }

    unsafe fn write_payload(&self, data: &[u8], timeout: &mut SpinTimeout) -> Result<(), HalError> {
        for &byte in data {
            self.wait_sr1(SR1_TXE, timeout)?;
            raw::write32(self.base() + DR, u32::from(byte));
        }
        if !data.is_empty() {
            self.wait_sr1(SR1_BTF, timeout)?;
        }
        Ok(())
    }

    unsafe fn read_payload(
        &self,
        buffer: &mut [u8],
        timeout: &mut SpinTimeout,
    ) -> Result<(), HalError> {
        let len = buffer.len();
        for (index, slot) in buffer.iter_mut().enumerate() {
            if index + 1 == len {
                raw::clear_bits(self.base() + CR1, CR1_ACK);
            } else {
                raw::set_bits(self.base() + CR1, CR1_ACK);
            }
            self.wait_sr1(SR1_RXNE, timeout)?;
            *slot = raw::read32(self.base() + DR) as u8;
        }
        Ok(())
    }

    unsafe fn begin(&self) -> Result<SpinTimeout, HalError> {
        self.require_enabled()?;
        let mut timeout = self.timeout();
        self.start_condition(&mut timeout)?;
        Ok(timeout)
    }

    unsafe fn probe(&self, address: u16) -> Result<(), HalError> {
        if self.config.addr_mode != I2cAddrMode::Addr7Bit {
            return Err(HalError::NotSupported);
        }
        if address > 0x7F {
            return Err(HalError::InvalidArgument);
        }
        self.require_enabled()?;
        let mut timeout = self.timeout();
        let result = self
            .start_condition(&mut timeout)
            .and_then(|()| self.send_address(address, false, &mut timeout));
        self.abort();
        result
    }
}

impl I2c for I2cMaster {
    type Error = HalError;

    fn configure(&mut self, config: I2cConfig) -> Result<(), Self::Error> {
        if config.addr_mode == I2cAddrMode::Addr10Bit && config.address > 0x3FF {
            return Err(HalError::InvalidArgument);
        }
        if config.addr_mode == I2cAddrMode::Addr7Bit && config.address > 0x7F {
            return Err(HalError::InvalidArgument);
        }

        let speed_hz = match config.speed {
            I2cSpeed::Standard => 100_000,
            I2cSpeed::Fast => 400_000,
            I2cSpeed::FastPlus | I2cSpeed::HighSpeed => return Err(HalError::NotSupported),
        };
        let freq_mhz = (self.clock_hz / 1_000_000).clamp(1, 50);
        let base = self.base();

        let ccr = match config.speed {
            I2cSpeed::Standard => self.clock_hz / (2 * speed_hz),
            I2cSpeed::Fast => self.clock_hz / (3 * speed_hz),
            I2cSpeed::FastPlus | I2cSpeed::HighSpeed => unreachable!(),
        };
        if !(4..=0x0FFF).contains(&ccr) {
            return Err(HalError::InvalidArgument);
        }
        let trise = match config.speed {
            I2cSpeed::Standard => freq_mhz + 1,
            I2cSpeed::Fast => (freq_mhz * 300 / 1000) + 1,
            I2cSpeed::FastPlus | I2cSpeed::HighSpeed => unreachable!(),
        };

        unsafe {
            raw::clear_bits(base + CR1, CR1_PE);
            raw::write32(base + CR2, freq_mhz);
            let ccr_value = match config.speed {
                I2cSpeed::Fast => ccr | CCR_FS,
                _ => ccr,
            };
            raw::write32(base + CCR, ccr_value);
            raw::write32(base + TRISE, trise);
            raw::write32(base + FLTR, 0);
            raw::write32(base + CR1, CR1_PE | CR1_ACK);
        }

        self.config = config;
        Ok(())
    }

    fn write(&mut self, address: u16, data: &[u8]) -> Result<(), Self::Error> {
        unsafe {
            let mut timeout = self.begin()?;
            let result = self
                .send_address(address, false, &mut timeout)
                .and_then(|()| self.write_payload(data, &mut timeout))
                .and_then(|()| self.stop_condition(&mut timeout));
            if result.is_err() {
                self.abort();
            }
            result
        }
    }

    fn read(&mut self, address: u16, buffer: &mut [u8]) -> Result<(), Self::Error> {
        unsafe {
            let mut timeout = self.begin()?;
            let result = self
                .send_address(address, true, &mut timeout)
                .and_then(|()| self.read_payload(buffer, &mut timeout))
                .and_then(|()| self.stop_condition(&mut timeout));
            if result.is_err() {
                self.abort();
            }
            result
        }
    }

    fn write_read(
        &mut self,
        address: u16,
        write: &[u8],
        read: &mut [u8],
    ) -> Result<(), Self::Error> {
        unsafe {
            let mut timeout = self.begin()?;
            let result = self
                .send_address(address, false, &mut timeout)
                .and_then(|()| self.write_payload(write, &mut timeout))
                .and_then(|()| self.generate_start(&mut timeout))
                .and_then(|()| self.send_address(address, true, &mut timeout))
                .and_then(|()| self.read_payload(read, &mut timeout))
                .and_then(|()| self.stop_condition(&mut timeout));
            if result.is_err() {
                self.abort();
            }
            result
        }
    }

    fn scan(&self) -> Result<Vec<u8>, Self::Error> {
        let mut found = Vec::with_capacity(112);
        for address in 0x08..=0x77 {
            match unsafe { self.probe(address) } {
                Ok(()) => found.push(address as u8),
                Err(HalError::Nack) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(found)
    }

    fn write_dma(&mut self, _address: u16, _data: &[u8]) -> Result<(), Self::Error> {
        Err(HalError::NotSupported)
    }

    fn read_dma(&mut self, _address: u16, _buffer: &mut [u8]) -> Result<(), Self::Error> {
        Err(HalError::NotSupported)
    }

    fn write_read_dma(
        &mut self,
        _address: u16,
        _write: &[u8],
        _read: &mut [u8],
    ) -> Result<(), Self::Error> {
        Err(HalError::NotSupported)
    }
}

/// Bounded polling helper. One tick corresponds to one bus poll; the budget
/// is scaled from the configured timeout so a stuck bus cannot spin forever.
struct SpinTimeout {
    remaining: u32,
}

impl SpinTimeout {
    fn new(timeout_ms: u32, clock_hz: u32) -> Self {
        let per_ms = (clock_hz / 1_000).max(1);
        Self {
            remaining: timeout_ms.max(1).saturating_mul(per_ms).max(1),
        }
    }

    fn tick(&mut self) -> Result<(), HalError> {
        if self.remaining == 0 {
            return Err(HalError::Timeout);
        }
        self.remaining -= 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn register_offsets_match_rm0390() {
        assert_eq!(offset_of!(raw::I2cBlock, cr1), 0x00);
        assert_eq!(offset_of!(raw::I2cBlock, cr2), 0x04);
        assert_eq!(offset_of!(raw::I2cBlock, dr), 0x10);
        assert_eq!(offset_of!(raw::I2cBlock, sr1), 0x14);
        assert_eq!(offset_of!(raw::I2cBlock, sr2), 0x18);
        assert_eq!(offset_of!(raw::I2cBlock, ccr), 0x1C);
        assert_eq!(offset_of!(raw::I2cBlock, trise), 0x20);
        assert_eq!(offset_of!(raw::I2cBlock, fltr), 0x24);
        assert_eq!(size_of::<raw::I2cBlock>(), 0x28);
    }

    #[test]
    fn spin_timeout_is_bounded() {
        let mut timeout = SpinTimeout::new(1, 16_000_000);
        for _ in 0..16_000 {
            timeout.tick().unwrap();
        }
        assert_eq!(timeout.tick(), Err(HalError::Timeout));
    }
}
