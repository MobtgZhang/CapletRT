//! Reset & clock control

use crate::raw::{self, f446};
use core::ptr::{read_volatile, write_volatile};

/// RCC helper.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rcc;

// SAFETY: Single hardware register block, all access guarded by
// our own atomic primitives when necessary. For clock registers
// the read-modify-write races are tolerated (the M1 phase does
// this with interrupts disabled in the boot path; the M2 phase
// moves to atomics).
unsafe impl Send for Rcc {}

impl Rcc {
    /// Take the singleton — unsafe because it assumes only one
    /// caller (typically `Peripherals::take()` at boot).
    ///
    /// # Safety
    /// The caller must ensure that no other code concurrently mutates the
    /// RCC clock-enable and reset registers.
    pub unsafe fn take() -> Self {
        Self
    }

    /// Enable the GPIOA clock.
    pub fn enable_gpioa(&mut self) {
        unsafe {
            raw::set_bits(f446::RCC + 0x30, 1 << 0);
        }
    }

    /// Enable the GPIOB clock.
    pub fn enable_gpiob(&mut self) {
        unsafe {
            raw::set_bits(f446::RCC + 0x30, 1 << 1);
        }
    }

    /// Enable the USART2 clock (APB1 bit 17).
    pub fn enable_usart2(&mut self) {
        unsafe {
            raw::set_bits(f446::RCC + 0x40, 1 << 17);
        }
    }

    /// Enable the USART1 clock (APB2 bit 4).
    pub fn enable_usart1(&mut self) {
        unsafe {
            raw::set_bits(f446::RCC + 0x44, 1 << 4);
        }
    }

    /// Enable the TIM2 clock (APB1 bit 0).
    pub fn enable_tim2(&mut self) {
        unsafe {
            raw::set_bits(f446::RCC + 0x40, 1 << 0);
        }
    }

    /// Enable an I2C peripheral clock.
    ///
    /// I2C1/2/3 are on APB1 bits 21/22/23. Returns `false` for an
    /// unsupported instance number.
    pub fn enable_i2c(&mut self, instance: u8) -> bool {
        let bit = match instance {
            1 => 21,
            2 => 22,
            3 => 23,
            _ => return false,
        };
        unsafe { raw::set_bits(f446::RCC + 0x40, 1 << bit) };
        true
    }

    /// Enable an SPI peripheral clock.
    ///
    /// SPI2/3 are on APB1 bits 14/15; SPI1/4/5/6 are on APB2 bits
    /// 12/13/20/21. Returns `false` for an unsupported instance number.
    pub fn enable_spi(&mut self, instance: u8) -> bool {
        let (offset, bit) = match instance {
            1 => (0x44, 12),
            2 => (0x40, 14),
            3 => (0x40, 15),
            4 => (0x44, 13),
            5 => (0x44, 20),
            6 => (0x44, 21),
            _ => return false,
        };
        unsafe { raw::set_bits(f446::RCC + offset, 1 << bit) };
        true
    }

    /// Enable the I2C1 clock.
    pub fn enable_i2c1(&mut self) {
        let _ = self.enable_i2c(1);
    }

    /// Enable the I2C2 clock.
    pub fn enable_i2c2(&mut self) {
        let _ = self.enable_i2c(2);
    }

    /// Enable the I2C3 clock.
    pub fn enable_i2c3(&mut self) {
        let _ = self.enable_i2c(3);
    }

    /// Enable the SPI1 clock.
    pub fn enable_spi1(&mut self) {
        let _ = self.enable_spi(1);
    }

    /// Enable the SPI2 clock.
    pub fn enable_spi2(&mut self) {
        let _ = self.enable_spi(2);
    }

    /// Enable the SPI3 clock.
    pub fn enable_spi3(&mut self) {
        let _ = self.enable_spi(3);
    }

    /// Enable the SPI4 clock.
    pub fn enable_spi4(&mut self) {
        let _ = self.enable_spi(4);
    }

    /// Enable the SPI5 clock.
    pub fn enable_spi5(&mut self) {
        let _ = self.enable_spi(5);
    }

    /// Enable the SPI6 clock.
    pub fn enable_spi6(&mut self) {
        let _ = self.enable_spi(6);
    }

    /// Configure HSI as the system clock at 16 MHz.
    pub fn hsi_16mhz(&mut self) {
        unsafe {
            // Set HSI as SYSCLK (SW = 00), AHB = 1 (HPRE = 0xxx),
            // APB1/2 = 1 (no prescaler).
            let mut cfgr = read_volatile((f446::RCC as *const u32).add(2));
            cfgr &= !0x3;
            write_volatile((f446::RCC as *mut u32).add(2), cfgr);
        }
    }
}
