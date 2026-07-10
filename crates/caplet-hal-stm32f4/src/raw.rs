//! Raw MMIO layouts for STM32F4.
//!
//! We deliberately avoid the official `stm32f4` PAC so this HAL
//! can be built into boards that already vendor their own. Only the
//! peripherals CapletRT itself uses are described here, and they
//! are kept in `repr(C)` structs that match the reference manual.

use core::ptr::{read_volatile, write_volatile};

/// Read a 32-bit MMIO register.
#[inline]
pub unsafe fn read32(addr: usize) -> u32 {
    read_volatile(addr as *const u32)
}

/// Write a 32-bit MMIO register.
#[inline]
pub unsafe fn write32(addr: usize, value: u32) {
    write_volatile(addr as *mut u32, value);
}

/// Set `bits` in the 32-bit MMIO register at `addr`.
#[inline]
pub unsafe fn set_bits(addr: usize, bits: u32) {
    let v = read32(addr);
    write32(addr, v | bits);
}

/// Clear `bits` in the 32-bit MMIO register at `addr`.
#[inline]
pub unsafe fn clear_bits(addr: usize, bits: u32) {
    let v = read32(addr);
    write32(addr, v & !bits);
}

/// GPIO port register block (RM0390 §8.4).
#[repr(C)]
pub struct GpioBlock {
    /// Mode register.
    pub moder: u32,
    /// Output type register.
    pub otyper: u32,
    /// Output speed register.
    pub ospeedr: u32,
    /// Pull-up/pull-down register.
    pub pupdr: u32,
    /// Input data register.
    pub idr: u32,
    /// Output data register.
    pub odr: u32,
    /// Bit set/reset register (writes only).
    pub bsrr: u32,
    /// Bit reset register (writes only).
    pub brr: u32,
    /// Alternate function high register.
    pub afrh: u32,
    /// Alternate function low register.
    pub afrl: u32,
}

/// USART register block (RM0390 §30.6).
#[repr(C)]
pub struct UsartBlock {
    /// Status register.
    pub sr: u32,
    /// Data register.
    pub dr: u32,
    /// Baud rate register.
    pub brr: u32,
    /// Control register 1.
    pub cr1: u32,
    /// Control register 2.
    pub cr2: u32,
    /// Control register 3.
    pub cr3: u32,
    /// Guard time and prescaler register.
    pub gtpr: u32,
}

/// RCC register block (RM0390 §6.3).
#[repr(C)]
pub struct RccBlock {
    /// Clock control register.
    pub cr: u32,
    /// PLL configuration register.
    pub pllcfgr: u32,
    /// Clock configuration register.
    pub cfgr: u32,
    /// Clock interrupt register.
    pub cir: u32,
    /// AHB1 peripheral reset register.
    pub ahb1rstr: u32,
    _pad0: u32,
    /// APB1 peripheral reset register.
    pub apb1rstr: u32,
    /// APB2 peripheral reset register.
    pub apb2rstr: u32,
    _pad1: [u32; 2],
    /// AHB1 peripheral clock enable register.
    pub ahb1enr: u32,
    _pad2: u32,
    /// APB1 peripheral clock enable register.
    pub apb1enr: u32,
    /// APB2 peripheral clock enable register.
    pub apb2enr: u32,
    _pad3: [u32; 2],
    /// AHB1 peripheral clock enable in low-power mode register.
    pub ahb1lpenr: u32,
    _pad4: u32,
    /// APB1 peripheral clock enable in low-power mode register.
    pub apb1lpenr: u32,
    /// APB2 peripheral clock enable in low-power mode register.
    pub apb2lpenr: u32,
    _pad4p: [u32; 2],
    /// Backup domain control register.
    pub bdcr: u32,
    /// Clock control & status register.
    pub csr: u32,
    _pad5: [u32; 2],
    /// Spread spectrum clock generation register.
    pub sscgr: u32,
    /// PLL I2S configuration register.
    pub plli2scfgr: u32,
    /// PLL PLLSAI configuration register.
    pub pllsaicfgr: u32,
    /// Dedicated clock configuration register.
    pub dckcfgr: u32,
    /// clocks gated enable register.
    pub ckgatenr: u32,
    /// Dedicated clock configuration register 2.
    pub dckcfgr2: u32,
}

/// TIM timer block (subset used by HAL).
#[repr(C)]
pub struct TimBlock {
    /// Control register 1.
    pub cr1: u32,
    /// Control register 2.
    pub cr2: u32,
    /// Slave mode control register.
    pub smcr: u32,
    /// DMA/Interrupt enable register.
    pub dier: u32,
    /// Status register.
    pub sr: u32,
    /// Event generation register.
    pub egr: u32,
    /// Capture/compare mode register 1.
    pub ccmr1: u32,
    /// Capture/compare mode register 2.
    pub ccmr2: u32,
    /// Capture/compare enable register.
    pub ccer: u32,
    /// Counter.
    pub cnt: u32,
    /// Prescaler.
    pub psc: u32,
    /// Auto-reload register.
    pub arr: u32,
    _pad: u32,
    /// Capture/compare register 1.
    pub ccr1: u32,
    /// Capture/compare register 2.
    pub ccr2: u32,
    /// Capture/compare register 3.
    pub ccr3: u32,
    /// Capture/compare register 4.
    pub ccr4: u32,
    _pad2: [u32; 12],
    /// Alternate function option register 1.
    pub or: u32,
}

/// FLASH register block (RM0390 §3.7).
#[repr(C)]
pub struct FlashBlock {
    /// Access control register.
    pub acr: u32,
    /// Key register.
    pub keyr: u32,
    /// Option key register.
    pub optkeyr: u32,
    /// Status register.
    pub sr: u32,
    /// Control register.
    pub cr: u32,
    /// Option control register.
    pub optcr: u32,
    /// Option control register 1.
    pub optcr1: u32,
}

/// Compile-time base addresses for STM32F446 (the chip we target).
pub mod f446 {
    /// GPIOA
    pub const GPIOA: usize = 0x4002_0000;
    /// GPIOB
    pub const GPIOB: usize = 0x4002_0400;
    /// GPIOC
    pub const GPIOC: usize = 0x4002_0800;
    /// GPIOD
    pub const GPIOD: usize = 0x4002_0C00;
    /// GPIOE
    pub const GPIOE: usize = 0x4002_1000;
    /// GPIOF
    pub const GPIOF: usize = 0x4002_1400;
    /// GPIOG
    pub const GPIOG: usize = 0x4002_1800;
    /// GPIOH
    pub const GPIOH: usize = 0x4002_1C00;

    /// USART1
    pub const USART1: usize = 0x4001_1000;
    /// USART2
    pub const USART2: usize = 0x4000_4400;
    /// USART3
    pub const USART3: usize = 0x4000_4800;
    /// UART4
    pub const UART4: usize = 0x4000_4C00;
    /// UART5
    pub const UART5: usize = 0x4000_5000;
    /// USART6
    pub const USART6: usize = 0x4001_1400;

    /// RCC base
    pub const RCC: usize = 0x4002_3800;
    /// FLASH base
    pub const FLASH: usize = 0x4002_3C00;

    /// TIM2
    pub const TIM2: usize = 0x4000_0000;
    /// TIM3
    pub const TIM3: usize = 0x4000_0400;
    /// TIM4
    pub const TIM4: usize = 0x4000_0800;
    /// TIM5
    pub const TIM5: usize = 0x4000_0C00;
    /// TIM9
    pub const TIM9: usize = 0x4001_4000;
    /// TIM10
    pub const TIM10: usize = 0x4001_4400;
    /// TIM11
    pub const TIM11: usize = 0x4001_4800;
}