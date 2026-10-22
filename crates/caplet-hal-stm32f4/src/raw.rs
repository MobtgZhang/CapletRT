//! Raw MMIO layouts for STM32F4.
//!
//! We deliberately avoid the official `stm32f4` PAC so this HAL
//! can be built into boards that already vendor their own. Only the
//! peripherals CapletRT itself uses are described here, and they
//! are kept in `repr(C)` structs that match the reference manual.

use core::ptr::{read_volatile, write_volatile};

/// Read a 32-bit MMIO register.
///
/// # Safety
/// `addr` must be a valid, readable, 4-byte-aligned MMIO address that
/// remains mapped for the duration of the call. Volatile MMIO access must
/// not be modelled by a Rust reference.
#[inline]
pub unsafe fn read32(addr: usize) -> u32 {
    read_volatile(addr as *const u32)
}

/// Write a 32-bit MMIO register.
///
/// # Safety
/// `addr` must be a valid, writable, 4-byte-aligned MMIO address that
/// remains mapped for the duration of the call, and the caller must have
/// exclusive ownership of the peripheral (or otherwise coordinate writes).
#[inline]
pub unsafe fn write32(addr: usize, value: u32) {
    write_volatile(addr as *mut u32, value);
}

/// Set `bits` in the 32-bit MMIO register at `addr`.
///
/// # Safety
/// Same contract as [`write32`]; the read-modify-write sequence must be
/// coordinated with every other accessor of the same register.
#[inline]
pub unsafe fn set_bits(addr: usize, bits: u32) {
    let v = read32(addr);
    write32(addr, v | bits);
}

/// Clear `bits` in the 32-bit MMIO register at `addr`.
///
/// # Safety
/// Same contract as [`write32`]; the read-modify-write sequence must be
/// coordinated with every other accessor of the same register.
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
    /// Configuration lock register.
    pub lckr: u32,
    /// Alternate function low register.
    pub afrl: u32,
    /// Alternate function high register.
    pub afrh: u32,
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
    /// AHB2 peripheral reset register.
    pub ahb2rstr: u32,
    /// AHB3 peripheral reset register.
    pub ahb3rstr: u32,
    _pad0: u32,
    /// APB1 peripheral reset register.
    pub apb1rstr: u32,
    /// APB2 peripheral reset register.
    pub apb2rstr: u32,
    _pad1: [u32; 2],
    /// AHB1 peripheral clock enable register.
    pub ahb1enr: u32,
    /// AHB2 peripheral clock enable register.
    pub ahb2enr: u32,
    /// AHB3 peripheral clock enable register.
    pub ahb3enr: u32,
    _pad2: u32,
    /// APB1 peripheral clock enable register.
    pub apb1enr: u32,
    /// APB2 peripheral clock enable register.
    pub apb2enr: u32,
    _pad3: [u32; 2],
    /// AHB1 peripheral clock enable in low-power mode register.
    pub ahb1lpenr: u32,
    /// AHB2 peripheral clock enable in low-power mode register.
    pub ahb2lpenr: u32,
    /// AHB3 peripheral clock enable in low-power mode register.
    pub ahb3lpenr: u32,
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
    /// Repetition counter register.
    pub rcr: u32,
    /// Capture/compare register 1.
    pub ccr1: u32,
    /// Capture/compare register 2.
    pub ccr2: u32,
    /// Capture/compare register 3.
    pub ccr3: u32,
    /// Capture/compare register 4.
    pub ccr4: u32,
    /// Break and dead-time register.
    pub bdtr: u32,
    /// DMA control register.
    pub dcr: u32,
    /// DMA address for full transfer.
    pub dmar: u32,
    /// Alternate function option register 1.
    pub or: u32,
}

/// I2C register block (RM0390 §27.6).
#[repr(C)]
pub struct I2cBlock {
    /// Control register 1.
    pub cr1: u32,
    /// Control register 2.
    pub cr2: u32,
    /// Own address register 1.
    pub oar1: u32,
    /// Own address register 2.
    pub oar2: u32,
    /// Data register.
    pub dr: u32,
    /// Status register 1.
    pub sr1: u32,
    /// Status register 2.
    pub sr2: u32,
    /// Clock control register.
    pub ccr: u32,
    /// TRISE register.
    pub trise: u32,
    /// Filter register (F4 only).
    pub fltr: u32,
}

/// SPI register block (RM0390 §28.5).
#[repr(C)]
pub struct SpiBlock {
    /// Control register 1.
    pub cr1: u32,
    /// Control register 2.
    pub cr2: u32,
    /// Status register.
    pub sr: u32,
    /// Data register.
    pub dr: u32,
    /// CRC polynomial register.
    pub crcpr: u32,
    /// RX CRC register.
    pub rxcrcr: u32,
    /// TX CRC register.
    pub txcrcr: u32,
    /// I2S configuration register.
    pub i2scfgr: u32,
    /// I2S prescaler register.
    pub i2spr: u32,
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

    /// I2C1 (APB1)
    pub const I2C1: usize = 0x4000_5400;
    /// I2C2 (APB1)
    pub const I2C2: usize = 0x4000_5800;
    /// I2C3 (APB1)
    pub const I2C3: usize = 0x4000_5C00;

    /// SPI1 (APB2)
    pub const SPI1: usize = 0x4001_3000;
    /// SPI2 (APB1)
    pub const SPI2: usize = 0x4000_3800;
    /// SPI3 (APB1)
    pub const SPI3: usize = 0x4000_3C00;
    /// SPI4 (APB2)
    pub const SPI4: usize = 0x4001_3400;
    /// SPI5 (APB2)
    pub const SPI5: usize = 0x4001_5000;
    /// SPI6 (APB2)
    pub const SPI6: usize = 0x4001_5400;

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

/// Ethernet MAC register block (STM32F4).
///
/// Base address: 0x4002_8000.
///
/// The layout is a direct transcription of RM0390; individual bitfield
/// documentation is intentionally kept in the reference manual.
#[allow(missing_docs)]
pub mod eth {
    /// Ethernet MAC/DMA register block.
    #[allow(missing_docs)]
    #[repr(C)]
    pub struct RegisterBlock {
        // MAC registers
        pub maccr: u32,     // 0x0000: MAC Configuration Register
        pub macffr: u32,    // 0x0004: MAC Frame Filter Register
        pub machthr: u32,   // 0x0008: MAC Hash Table High Register
        pub machtlr: u32,   // 0x000C: MAC Hash Table Low Register
        pub macmiiar: u32,  // 0x0010: MAC MII Address Register
        pub macmiidr: u32,  // 0x0014: MAC MII Data Register
        pub macfcr: u32,    // 0x0018: MAC Flow Control Register
        pub macvlantr: u32, // 0x001C: MAC VLAN Tag Register
        // Reserved 0x0020-0x002C
        pub reserved0: [u32; 3], // 0x0020-0x002C
        pub macvr: u32,          // 0x0030: MAC Version Register
        pub macdr: u32,          // 0x0034: MAC Debug Register
        // Reserved 0x0038-0x003C
        pub reserved1: [u32; 2], // 0x0038-0x003C
        // MAC Remote Wake-Up Frame Filter Register
        pub macrwuffr: u32, // 0x0040: MAC Remote Wake-Up Frame Filter Register
        // Reserved 0x0044-0x0047
        pub reserved2: [u32; 1], // 0x0044-0x0047
        // MAC PMT Control/Status Register
        pub macpcsr: u32, // 0x0048: MAC PMT Control/Status Register
        // Reserved 0x004C-0x0067
        pub reserved3: [u32; 7], // 0x004C-0x0067
        // MAC Interrupt Status Register
        pub macisr: u32, // 0x0068: MAC Interrupt Status Register
        // MAC Interrupt Mask Register
        pub macimr: u32, // 0x006C: MAC Interrupt Mask Register
        // MAC Address0 High Register
        pub maca0hr: u32, // 0x0070: MAC Address0 High Register
        // MAC Address0 Low Register
        pub maca0lr: u32, // 0x0074: MAC Address0 Low Register
        // MAC Address1 High Register
        pub maca1hr: u32, // 0x0078: MAC Address1 High Register
        // MAC Address1 Low Register
        pub maca1lr: u32, // 0x007C: MAC Address1 Low Register
        // MAC Address2 High Register
        pub maca2hr: u32, // 0x0080: MAC Address2 High Register
        // MAC Address2 Low Register
        pub maca2lr: u32, // 0x0084: MAC Address2 Low Register
        // MAC Address3 High Register
        pub maca3hr: u32, // 0x0088: MAC Address3 High Register
        // MAC Address3 Low Register
        pub maca3lr: u32, // 0x008C: MAC Address3 Low Register
        // MAC Address4 High Register
        pub maca4hr: u32, // 0x0090: MAC Address4 High Register
        // MAC Address4 Low Register
        pub maca4lr: u32, // 0x0094: MAC Address4 Low Register
        // MAC Address5 High Register
        pub maca5hr: u32, // 0x0098: MAC Address5 High Register
        // MAC Address5 Low Register
        pub maca5lr: u32, // 0x009C: MAC Address5 Low Register
        // Reserved 0x00A0-0x00FF
        pub reserved4: [u32; 24], // 0x00A0-0x00FF
        // DMA registers (starting at 0x40028200)
        pub dma_bmr: u32,    // 0x0200: DMA Bus Mode Register
        pub dma_tpdr: u32,   // 0x0204: DMA Transmit Poll Demand Register
        pub dma_rpdr: u32,   // 0x0208: DMA Receive Poll Demand Register
        pub dma_rdlar: u32,  // 0x020C: DMA Receive Descriptor List Address Register
        pub dma_tdlar: u32,  // 0x0210: DMA Transmit Descriptor List Address Register
        pub dma_sr: u32,     // 0x0214: DMA Status Register
        pub dma_omr: u32,    // 0x0218: DMA Operation Mode Register
        pub dma_ier: u32,    // 0x021C: DMA Interrupt Enable Register
        pub dma_mfbocr: u32, // 0x0220: DMA Missed Frame and Buffer Overflow Counter Register
        pub dma_rswt: u32,   // 0x0224: DMA Receive Watchdog Timer Register
        // Reserved 0x0228-0x023B
        pub reserved5: [u32; 5], // 0x0228-0x023B
        pub dma_cfbocr: u32,     // 0x023C: DMA Current Bus Transmit Buffer Offset Register
        // Reserved 0x0240-0x025B
        pub reserved6: [u32; 7], // 0x0240-0x025B
        pub dma_cbtr: u32,       // 0x025C: DMA Current Bus Transmit Descriptor Register
    }
}
