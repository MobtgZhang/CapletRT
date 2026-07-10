//! USART driver

use crate::raw::{self, f446};
use caplet_hal::uart::{Uart, UartConfig};
use caplet_hal::HalError;

/// Owning handle to a USART peripheral.
pub struct Usart {
    base: *mut raw::UsartBlock,
    /// APB clock in Hz used for BRR calculation.
    clock_hz: u32,
}

unsafe impl Send for Usart {}

impl Usart {
    /// Take a USART2 (which on Nucleo-F446ZE is wired to the
    /// virtual COM port on ST-Link).
    pub unsafe fn usart2(clock_hz: u32) -> Self {
        Self {
            base: f446::USART2 as *mut raw::UsartBlock,
            clock_hz,
        }
    }

    /// Take USART1.
    pub unsafe fn usart1(clock_hz: u32) -> Self {
        Self {
            base: f446::USART1 as *mut raw::UsartBlock,
            clock_hz,
        }
    }
}

impl Uart for Usart {
    type Error = HalError;

    fn configure(&mut self, config: UartConfig) -> Result<(), Self::Error> {
        let base = self.base as usize;
        unsafe {
            // 1. Enable USART (UE bit in CR1).
            raw::set_bits(base + 0x0C, 1 << 13);
            // 2. Set word length (M0/M1 in CR1): 8-bit → 0.
            raw::clear_bits(base + 0x0C, 1 << 12);
            // 3. Set baud rate.
            let brr = self.clock_hz / config.baud_rate as u32;
            raw::write32(base + 0x08, brr);
            // 4. Enable transmitter and receiver.
            raw::set_bits(base + 0x0C, (1 << 3) | (1 << 2));
        }
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> Result<usize, Self::Error> {
        let base = self.base as usize;
        let mut written = 0;
        for &b in data {
            unsafe {
                // Wait for TXE (bit 7 of SR).
                while (raw::read32(base) & (1 << 7)) == 0 {
                    core::hint::spin_loop();
                }
                raw::write32(base + 0x04, b as u32);
                written += 1;
            }
        }
        Ok(written)
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        let base = self.base as usize;
        let mut count = 0;
        for slot in buffer.iter_mut() {
            unsafe {
                if (raw::read32(base) & (1 << 5)) == 0 {
                    // RXNE not set → return how many we have so far.
                    break;
                }
                *slot = (raw::read32(base + 0x04) & 0xFF) as u8;
                count += 1;
            }
        }
        Ok(count)
    }

    fn available(&self) -> bool {
        let base = self.base as usize;
        unsafe { (raw::read32(base) & (1 << 5)) != 0 }
    }

    fn tx_empty(&self) -> bool {
        let base = self.base as usize;
        unsafe { (raw::read32(base) & (1 << 7)) != 0 }
    }
}

impl core::fmt::Write for Usart
where
    Usart: Uart<Error = HalError>,
{
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let _ = self.write_all(s.as_bytes());
        Ok(())
    }
}