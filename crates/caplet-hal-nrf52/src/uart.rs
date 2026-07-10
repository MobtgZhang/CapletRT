//! UARTE driver (no-PAC, MMIO only).

use crate::raw::{self, nrf52832};
use caplet_hal::uart::{Uart, UartConfig};
use caplet_hal::HalError;

/// UARTE instance.
pub struct Uarte {
    base: *mut raw::UarteBlock,
}

unsafe impl Send for Uarte {}

impl Uarte {
    /// Take UARTE0.
    pub unsafe fn uarte0() -> Self {
        Self {
            base: nrf52832::UARTE0 as *mut raw::UarteBlock,
        }
    }

    /// Take UARTE1.
    pub unsafe fn uarte1() -> Self {
        Self {
            base: nrf52832::UARTE1 as *mut raw::UarteBlock,
        }
    }
}

impl Uart for Uarte {
    type Error = HalError;

    fn configure(&mut self, _config: UartConfig) -> Result<(), Self::Error> {
        let base = self.base as usize;
        unsafe {
            // Enable (ENABLE register at offset 0x500, value = 8).
            raw::write32(base + 0x500, 8);
            // Baud rate: register at 0x524, value = baud_rate.
            // nRF52832 baud rate table: 9600 = 0x0027_5000
            raw::write32(base + 0x524, 0x0027_5000);
        }
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> Result<usize, Self::Error> {
        let base = self.base as usize;
        // Set TXD pointer, count, then trigger START task.
        unsafe {
            // EasyDMA TXD pointer is at offset 0x540 (PTR),
            // count at 0x548 (MAXCNT).
            let ptr_reg = (data.as_ptr() as u32) as usize;
            raw::write32(base + 0x540, ptr_reg as u32);
            raw::write32(base + 0x548, data.len() as u32);
            // Trigger START.
            raw::write32(base + 0x000, 1);
        }
        // Wait for ENDTX (offset 0x120).
        unsafe {
            while raw::read32(base + 0x120) == 0 {
                core::hint::spin_loop();
            }
            raw::write32(base + 0x120, 0);
        }
        Ok(data.len())
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        let base = self.base as usize;
        unsafe {
            let ptr_reg = (buffer.as_mut_ptr() as u32) as usize;
            raw::write32(base + 0x544, ptr_reg as u32);
            raw::write32(base + 0x54C, buffer.len() as u32);
            raw::write32(base + 0x000, 1);
            while raw::read32(base + 0x108) == 0 {
                core::hint::spin_loop();
            }
            raw::write32(base + 0x108, 0);
            // Real RX count is in the RXD.AMOUNT register (0x54C).
            let amount = raw::read32(base + 0x54C) as usize;
            Ok(amount.min(buffer.len()))
        }
    }

    fn available(&self) -> bool {
        let base = self.base as usize;
        unsafe { raw::read32(base + 0x108) != 0 }
    }

    fn tx_empty(&self) -> bool {
        let base = self.base as usize;
        unsafe { raw::read32(base + 0x120) != 0 }
    }
}

impl core::fmt::Write for Uarte {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let _ = self.write_all(s.as_bytes());
        Ok(())
    }
}