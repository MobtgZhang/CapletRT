//! Reset & clock control

use crate::raw::{self, f446};
use core::ptr::{read_volatile, write_volatile};

/// RCC helper.
pub struct Rcc {
    base: *mut raw::RccBlock,
}

// SAFETY: Single hardware register block, all access guarded by
// our own atomic primitives when necessary. For clock registers
// the read-modify-write races are tolerated (the M1 phase does
// this with interrupts disabled in the boot path; the M2 phase
// moves to atomics).
unsafe impl Send for Rcc {}

impl Rcc {
    /// Take the singleton — unsafe because it assumes only one
    /// caller (typically `Peripherals::take()` at boot).
    pub unsafe fn take() -> Self {
        Self {
            base: f446::RCC as *mut raw::RccBlock,
        }
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