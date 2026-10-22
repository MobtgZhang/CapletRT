//! Raw MMIO layouts for nRF52.
//!
//! We don't want a build-time dependency on a PAC, so this module
//! defines just the registers CapletRT needs and exposes them
//! behind [`read32`]/[`write32`]/[`set_bits`]/[`clear_bits`].

use core::ptr::{read_volatile, write_volatile};

/// Read a 32-bit MMIO register.
///
/// # Safety
///
/// `addr` must be a readable, 32-bit-aligned MMIO address that remains
/// mapped for the duration of the access.
#[inline]
pub unsafe fn read32(addr: usize) -> u32 {
    read_volatile(addr as *const u32)
}

/// Write a 32-bit MMIO register.
///
/// # Safety
///
/// `addr` must be a writable, 32-bit-aligned MMIO address that remains
/// mapped for the duration of the access.
#[inline]
pub unsafe fn write32(addr: usize, value: u32) {
    write_volatile(addr as *mut u32, value);
}

/// Set `bits` in the 32-bit MMIO register at `addr`.
///
/// # Safety
///
/// The same address requirements as [`read32`] and [`write32`] apply.
#[inline]
pub unsafe fn set_bits(addr: usize, bits: u32) {
    let v = read32(addr);
    write32(addr, v | bits);
}

/// Clear `bits` in the 32-bit MMIO register at `addr`.
///
/// # Safety
///
/// The same address requirements as [`read32`] and [`write32`] apply.
#[inline]
pub unsafe fn clear_bits(addr: usize, bits: u32) {
    let v = read32(addr);
    write32(addr, v & !bits);
}

/// GPIO port register block (nRF52832 Product Specification §6.8).
#[allow(dead_code)]
#[repr(C)]
pub struct GpioBlock {
    _reserved0: [u32; 0x504 / 4],
    /// Output value written as a whole register.
    pub out: u32,
    /// Write-one-to-set output bits.
    pub outset: u32,
    /// Write-one-to-clear output bits.
    pub outclr: u32,
    /// Pin input values.
    pub in_: u32,
    /// Pin direction (0=input, 1=output).
    pub dir: u32,
    /// Write-one-to-set direction bits.
    pub dirset: u32,
    /// Write-one-to-clear direction bits.
    pub dirclr: u32,
    /// Latched input values.
    pub latch: u32,
    /// Input detect mode.
    pub detectmode: u32,
    _reserved1: [u32; (0x700 - 0x528) / 4],
    /// Per-pin configuration (`PIN_CNF[0..32]`).
    pub pin_cnf: [u32; 32],
}

/// UARTE register block (nRF52832 Product Specification §6.35).
#[allow(dead_code)]
#[repr(C)]
pub struct UarteBlock {
    /// Start RX DMA task.
    pub tasks_startrx: u32,
    /// Stop RX task.
    pub tasks_stoprx: u32,
    /// Start TX DMA task.
    pub tasks_starttx: u32,
    /// Stop TX task.
    pub tasks_stoptx: u32,
    _reserved0: [u32; 3],
    /// Suspend UARTE.
    pub tasks_suspend: u32,
    _reserved1: [u32; (0x100 - 0x20) / 4],
    /// Clear-to-send event.
    pub events_cts: u32,
    /// Not-clear-to-send event.
    pub events_ncts: u32,
    /// A byte has been received.
    pub events_rxdrdy: u32,
    _reserved2: u32,
    /// RX EasyDMA buffer completed.
    pub events_endrx: u32,
    _reserved3: [u32; 2],
    /// A byte has been transmitted.
    pub events_txdrdy: u32,
    /// TX EasyDMA buffer completed.
    pub events_endtx: u32,
    /// UARTE error event.
    pub events_error: u32,
    _reserved4: [u32; (0x200 - 0x128) / 4],
    /// Shortcut configuration.
    pub shorts: u32,
    _reserved5: [u32; (0x304 - 0x204) / 4],
    /// Interrupt enable set.
    pub intenset: u32,
    /// Interrupt enable clear.
    pub intenclr: u32,
    _reserved6: [u32; (0x480 - 0x30C) / 4],
    /// Error source flags (write one to clear).
    pub errorsrc: u32,
    _reserved7: [u32; (0x500 - 0x484) / 4],
    /// Peripheral enable (`8` enables UARTE).
    pub enable: u32,
    _reserved7a: u32,
    /// Pin select for RTS.
    pub psel_rts: u32,
    /// Pin select for TXD.
    pub psel_txd: u32,
    /// Pin select for CTS.
    pub psel_cts: u32,
    /// Pin select for RXD.
    pub psel_rxd: u32,
    _reserved8: [u32; (0x524 - 0x518) / 4],
    /// Baud-rate selector value.
    pub baudrate: u32,
    _reserved9: [u32; (0x534 - 0x528) / 4],
    /// RX EasyDMA pointer.
    pub rxd_ptr: u32,
    /// RX EasyDMA maximum count.
    pub rxd_maxcnt: u32,
    /// Number of bytes received by EasyDMA.
    pub rxd_amount: u32,
    /// TX EasyDMA pointer.
    pub txd_ptr: u32,
    /// TX EasyDMA maximum count.
    pub txd_maxcnt: u32,
    /// Number of bytes transmitted by EasyDMA.
    pub txd_amount: u32,
    _reserved10: [u32; (0x56C - 0x54C) / 4],
    /// UARTE configuration (hardware flow control and parity).
    pub config: u32,
}

/// RTC register block (nRF52832 Product Specification §6.22).
#[allow(dead_code)]
#[repr(C)]
pub struct RtcBlock {
    /// Start RTC task.
    pub tasks_start: u32,
    /// Stop RTC task.
    pub tasks_stop: u32,
    /// Clear RTC task.
    pub tasks_clear: u32,
    /// Trigger counter overflow task.
    pub tasks_trigovrflw: u32,
    _reserved0: [u32; (0x100 - 0x10) / 4],
    /// Tick event.
    pub events_tick: u32,
    /// Overflow event.
    pub events_ovrflw: u32,
    _reserved1: [u32; (0x140 - 0x108) / 4],
    /// COMPARE events.
    pub events_compare: [u32; 4],
    _reserved2: [u32; (0x304 - 0x150) / 4],
    /// Interrupt enable set.
    pub intenset: u32,
    /// Interrupt enable clear.
    pub intenclr: u32,
    _reserved3: [u32; (0x504 - 0x30C) / 4],
    /// 24-bit counter value.
    pub counter: u32,
    /// RTC prescaler.
    pub prescaler: u32,
    _reserved4: [u32; (0x540 - 0x50C) / 4],
    /// Compare/capture registers.
    pub cc: [u32; 4],
}

/// TIMER register block (nRF52832 Product Specification §6.24).
#[allow(dead_code)]
#[repr(C)]
pub struct TimBlock {
    /// Start timer task.
    pub tasks_start: u32,
    /// Stop timer task.
    pub tasks_stop: u32,
    /// Count one tick task.
    pub tasks_count: u32,
    /// Clear timer task.
    pub tasks_clear: u32,
    _reserved0: [u32; (0x40 - 0x10) / 4],
    /// Capture tasks.
    pub tasks_capture: [u32; 6],
    _reserved1: [u32; (0x140 - 0x58) / 4],
    /// Compare events.
    pub events_compare: [u32; 6],
    _reserved2: [u32; (0x200 - 0x158) / 4],
    /// Shortcut configuration.
    pub shorts: u32,
    _reserved3: [u32; (0x304 - 0x204) / 4],
    /// Interrupt enable set.
    pub intenset: u32,
    /// Interrupt enable clear.
    pub intenclr: u32,
    _reserved4: [u32; (0x504 - 0x30C) / 4],
    /// Timer mode (0=timer, 1=counter).
    pub mode: u32,
    /// Bit width (0=8, 1=16, 2=24, 3=32 bits).
    pub bitmode: u32,
    _reserved5: u32,
    /// Prescaler (divides by `2^prescaler`).
    pub prescaler: u32,
    _reserved6: [u32; (0x540 - 0x514) / 4],
    /// Capture/compare registers.
    pub cc: [u32; 6],
}

/// TWIM register block (nRF52832 Product Specification §6.32).
#[allow(dead_code)]
#[repr(C)]
pub struct TwimBlock {
    /// Start RX task.
    pub tasks_startrx: u32,
    _reserved0: u32,
    /// Start TX task.
    pub tasks_starttx: u32,
    _reserved1: [u32; 2],
    /// Stop transaction task.
    pub tasks_stop: u32,
    _reserved2: u32,
    /// Suspend transaction task.
    pub tasks_suspend: u32,
    /// Resume transaction task.
    pub tasks_resume: u32,
    _reserved3: [u32; (0x104 - 0x24) / 4],
    /// Transaction stopped event.
    pub events_stopped: u32,
    _reserved4: [u32; (0x124 - 0x108) / 4],
    /// Error event.
    pub events_error: u32,
    _reserved5: [u32; (0x148 - 0x128) / 4],
    /// Transaction suspended event.
    pub events_suspended: u32,
    /// RX started event.
    pub events_rxstarted: u32,
    /// TX started event.
    pub events_txstarted: u32,
    _reserved6: [u32; (0x15C - 0x154) / 4],
    /// Last RX byte event.
    pub events_lastrx: u32,
    /// Last TX byte event.
    pub events_lasttx: u32,
    _reserved7: [u32; (0x200 - 0x164) / 4],
    /// Shortcut configuration.
    pub shorts: u32,
    _reserved8: [u32; (0x304 - 0x204) / 4],
    /// Interrupt enable set.
    pub intenset: u32,
    /// Interrupt enable clear.
    pub intenclr: u32,
    _reserved9: [u32; (0x4C4 - 0x30C) / 4],
    /// Error source flags (write one to clear).
    pub errorsrc: u32,
    _reserved10: [u32; (0x500 - 0x4C8) / 4],
    /// Peripheral enable (`6` enables TWIM).
    pub enable: u32,
    _reserved10a: u32,
    /// Pin select for SCL.
    pub psel_scl: u32,
    /// Pin select for SDA.
    pub psel_sda: u32,
    _reserved11: [u32; (0x524 - 0x510) / 4],
    /// I2C frequency selector.
    pub frequency: u32,
    _reserved12: [u32; (0x534 - 0x528) / 4],
    /// RX EasyDMA pointer.
    pub rxd_ptr: u32,
    /// RX EasyDMA maximum count.
    pub rxd_maxcnt: u32,
    /// Number of bytes received.
    pub rxd_amount: u32,
    /// TX EasyDMA pointer.
    pub txd_ptr: u32,
    /// TX EasyDMA maximum count.
    pub txd_maxcnt: u32,
    /// Number of bytes transmitted.
    pub txd_amount: u32,
    _reserved13: [u32; (0x588 - 0x54C) / 4],
    /// Target address for the next transaction.
    pub address: u32,
}

/// SPIM register block (nRF52832 Product Specification §6.30).
#[allow(dead_code)]
#[repr(C)]
pub struct SpimBlock {
    /// Start transaction task.
    pub tasks_start: u32,
    /// Stop transaction task.
    pub tasks_stop: u32,
    _reserved0: [u32; (0x1C - 0x08) / 4],
    /// Suspend transaction task.
    pub tasks_suspend: u32,
    /// Resume transaction task.
    pub tasks_resume: u32,
    _reserved1: [u32; (0x104 - 0x24) / 4],
    /// Transaction stopped event.
    pub events_stopped: u32,
    _reserved2: [u32; (0x110 - 0x108) / 4],
    /// RX EasyDMA buffer completed.
    pub events_endrx: u32,
    _reserved3: u32,
    /// Transaction completed event.
    pub events_end: u32,
    _reserved4: u32,
    /// TX EasyDMA buffer completed.
    pub events_endtx: u32,
    _reserved5: [u32; (0x200 - 0x124) / 4],
    /// Shortcut configuration.
    pub shorts: u32,
    _reserved6: [u32; (0x304 - 0x204) / 4],
    /// Interrupt enable set.
    pub intenset: u32,
    /// Interrupt enable clear.
    pub intenclr: u32,
    _reserved7: [u32; (0x500 - 0x30C) / 4],
    /// Peripheral enable (`7` enables SPIM).
    pub enable: u32,
    _reserved7a: u32,
    /// Pin select for SCK.
    pub psel_sck: u32,
    /// Pin select for MOSI.
    pub psel_mosi: u32,
    /// Pin select for MISO.
    pub psel_miso: u32,
    /// Pin select for CSN.
    pub psel_csn: u32,
    _reserved8: [u32; (0x524 - 0x518) / 4],
    /// SPI frequency selector.
    pub frequency: u32,
    _reserved9: [u32; (0x534 - 0x528) / 4],
    /// RX EasyDMA pointer.
    pub rxd_ptr: u32,
    /// RX EasyDMA maximum count.
    pub rxd_maxcnt: u32,
    /// Number of bytes received.
    pub rxd_amount: u32,
    /// TX EasyDMA pointer.
    pub txd_ptr: u32,
    /// TX EasyDMA maximum count.
    pub txd_maxcnt: u32,
    /// Number of bytes transmitted.
    pub txd_amount: u32,
    _reserved10: [u32; (0x554 - 0x54C) / 4],
    /// SPI mode configuration.
    pub config: u32,
    _reserved11: [u32; (0x5C0 - 0x558) / 4],
    /// Over-read character used for RX-only transfers.
    pub orc: u32,
}

/// Compile-time base addresses for nRF52832.
pub mod nrf52832 {
    /// GPIO P0.
    pub const GPIO_P0: usize = 0x5000_0000;

    /// UARTE0.
    pub const UARTE0: usize = 0x4000_2000;
    /// UARTE1 (available on packages that expose it).
    pub const UARTE1: usize = 0x4000_2800;

    /// RTC0.
    pub const RTC0: usize = 0x4000_1000;
    /// RTC1.
    pub const RTC1: usize = 0x4000_1100;
    /// RTC2.
    pub const RTC2: usize = 0x4000_1400;

    /// TIMER0.
    pub const TIMER0: usize = 0x4000_8000;
    /// TIMER1.
    pub const TIMER1: usize = 0x4000_8100;
    /// TIMER2.
    pub const TIMER2: usize = 0x4000_8200;

    /// TWIM0.
    pub const TWIM0: usize = 0x4000_3000;
    /// TWIM1.
    pub const TWIM1: usize = 0x4000_4000;

    /// SPIM0.
    pub const SPIM0: usize = 0x4000_3000;
    /// SPIM1.
    pub const SPIM1: usize = 0x4000_4000;
    /// SPIM2.
    pub const SPIM2: usize = 0x4000_B000;

    /// HFCLK start task.
    pub const CLOCK_HFCLKSTART: usize = 0x4000_0000;
    /// LFCLK start task.
    pub const CLOCK_LFCLKSTART: usize = 0x4000_0008;
}

/// Compile-time base addresses that only exist on nRF52840.
pub mod nrf52840 {
    /// GPIO P1.
    pub const GPIO_P1: usize = 0x5000_0300;
    /// SPIM3.
    pub const SPIM3: usize = 0x4002_3000;
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    #[test]
    fn gpio_layout_matches_product_specification() {
        assert_eq!(offset_of!(GpioBlock, out), 0x504);
        assert_eq!(offset_of!(GpioBlock, outset), 0x508);
        assert_eq!(offset_of!(GpioBlock, outclr), 0x50C);
        assert_eq!(offset_of!(GpioBlock, in_), 0x510);
        assert_eq!(offset_of!(GpioBlock, dir), 0x514);
        assert_eq!(offset_of!(GpioBlock, pin_cnf), 0x700);
    }

    #[test]
    fn uarte_layout_matches_product_specification() {
        assert_eq!(offset_of!(UarteBlock, tasks_startrx), 0x000);
        assert_eq!(offset_of!(UarteBlock, tasks_starttx), 0x008);
        assert_eq!(offset_of!(UarteBlock, events_rxdrdy), 0x108);
        assert_eq!(offset_of!(UarteBlock, events_endrx), 0x110);
        assert_eq!(offset_of!(UarteBlock, events_endtx), 0x120);
        assert_eq!(offset_of!(UarteBlock, events_error), 0x124);
        assert_eq!(offset_of!(UarteBlock, intenset), 0x304);
        assert_eq!(offset_of!(UarteBlock, enable), 0x500);
        assert_eq!(offset_of!(UarteBlock, psel_rxd), 0x514);
        assert_eq!(offset_of!(UarteBlock, baudrate), 0x524);
        assert_eq!(offset_of!(UarteBlock, rxd_ptr), 0x534);
        assert_eq!(offset_of!(UarteBlock, txd_ptr), 0x540);
        assert_eq!(offset_of!(UarteBlock, config), 0x56C);
    }

    #[test]
    fn rtc_and_timer_layouts_match_product_specification() {
        assert_eq!(offset_of!(RtcBlock, events_tick), 0x100);
        assert_eq!(offset_of!(RtcBlock, events_compare), 0x140);
        assert_eq!(offset_of!(RtcBlock, intenset), 0x304);
        assert_eq!(offset_of!(RtcBlock, counter), 0x504);
        assert_eq!(offset_of!(RtcBlock, prescaler), 0x508);
        assert_eq!(offset_of!(RtcBlock, cc), 0x540);

        assert_eq!(offset_of!(TimBlock, tasks_capture), 0x040);
        assert_eq!(offset_of!(TimBlock, events_compare), 0x140);
        assert_eq!(offset_of!(TimBlock, shorts), 0x200);
        assert_eq!(offset_of!(TimBlock, intenset), 0x304);
        assert_eq!(offset_of!(TimBlock, mode), 0x504);
        assert_eq!(offset_of!(TimBlock, bitmode), 0x508);
        assert_eq!(offset_of!(TimBlock, prescaler), 0x510);
        assert_eq!(offset_of!(TimBlock, cc), 0x540);
    }

    #[test]
    fn twim_and_spim_layouts_match_product_specification() {
        assert_eq!(offset_of!(TwimBlock, tasks_starttx), 0x008);
        assert_eq!(offset_of!(TwimBlock, tasks_stop), 0x014);
        assert_eq!(offset_of!(TwimBlock, events_stopped), 0x104);
        assert_eq!(offset_of!(TwimBlock, events_error), 0x124);
        assert_eq!(offset_of!(TwimBlock, events_lastrx), 0x15C);
        assert_eq!(offset_of!(TwimBlock, events_lasttx), 0x160);
        assert_eq!(offset_of!(TwimBlock, intenset), 0x304);
        assert_eq!(offset_of!(TwimBlock, errorsrc), 0x4C4);
        assert_eq!(offset_of!(TwimBlock, enable), 0x500);
        assert_eq!(offset_of!(TwimBlock, psel_scl), 0x508);
        assert_eq!(offset_of!(TwimBlock, psel_sda), 0x50C);
        assert_eq!(offset_of!(TwimBlock, frequency), 0x524);
        assert_eq!(offset_of!(TwimBlock, rxd_ptr), 0x534);
        assert_eq!(offset_of!(TwimBlock, txd_ptr), 0x540);
        assert_eq!(offset_of!(TwimBlock, address), 0x588);

        assert_eq!(offset_of!(SpimBlock, tasks_start), 0x000);
        assert_eq!(offset_of!(SpimBlock, tasks_stop), 0x004);
        assert_eq!(offset_of!(SpimBlock, events_stopped), 0x104);
        assert_eq!(offset_of!(SpimBlock, events_endrx), 0x110);
        assert_eq!(offset_of!(SpimBlock, events_end), 0x118);
        assert_eq!(offset_of!(SpimBlock, events_endtx), 0x120);
        assert_eq!(offset_of!(SpimBlock, intenset), 0x304);
        assert_eq!(offset_of!(SpimBlock, enable), 0x500);
        assert_eq!(offset_of!(SpimBlock, psel_csn), 0x514);
        assert_eq!(offset_of!(SpimBlock, frequency), 0x524);
        assert_eq!(offset_of!(SpimBlock, rxd_ptr), 0x534);
        assert_eq!(offset_of!(SpimBlock, txd_ptr), 0x540);
        assert_eq!(offset_of!(SpimBlock, config), 0x554);
        assert_eq!(offset_of!(SpimBlock, orc), 0x5C0);
    }
}
