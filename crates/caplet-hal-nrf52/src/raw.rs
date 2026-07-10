//! Raw MMIO layouts for nRF52.
//!
//! We don't want a build-time dependency on a PAC, so this module
//! defines just the registers CapletRT needs and exposes them
//! behind [`read32`]/[`write32`]/[`set_bits`]/[`clear_bits`].

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

/// GPIO port register block (subset, nRF52832 Product Spec §6.8).
#[repr(C)]
pub struct GpioBlock {
    _pad0: u32,
    /// Pin direction (0=in, 1=out) — bit per pin.
    pub dir: u32,
    _pad1: u32,
    /// Pin input sense configuration.
    pub in_: u32,
    _pad2: u32,
    /// Pin output drive configuration.
    pub out: u32,
    _pad3: u32,
    /// Pin output set.
    pub outset: u32,
    /// Pin output clear.
    pub outclr: u32,
    _pad4: [u32; 2],
    /// Pin input (read-only).
    pub in_pin: u32,
    _pad5: u32,
    /// Pin output drive strength.
    pub drv: u32,
    _pad6: [u32; 6],
    /// Pin pull-up/pull-down configuration.
    pub pull: u32,
}

/// UARTE register block (subset, nRF52832 Product Spec §6.35).
#[repr(C)]
pub struct UarteBlock {
    /// Task start.
    pub tasks_start: u32,
    /// Task stop.
    pub tasks_stop: u32,
    _pad0: [u32; 2],
    /// Task suspend.
    pub tasks_suspend: u32,
    _pad1: u32,
    /// Event started.
    pub events_started: u32,
    _pad2: [u32; 7],
    /// Event endrx.
    pub events_endrx: u32,
    _pad3: [u32; 3],
    /// Event endtx.
    pub events_endtx: u32,
    _pad4: [u32; 11],
    /// Event error.
    pub events_error: u32,
    _pad5: [u32; 7],
    /// Shortcuts.
    pub shorts: u32,
    _pad6: [u32; 63],
    /// Enable UARTE.
    pub enable: u32,
    _pad7: [u32; 1],
    /// Pin select for RTS, TXD, CTS, RXD.
    pub psel: [u32; 4],
    _pad8: [u32; 3],
    /// Baud rate.
    pub baudrate: u32,
    _pad9: [u32; 3],
    /// Configuration.
    pub config: u32,
}

/// RTC register block (nRF52832 Product Spec §6.22).
#[repr(C)]
pub struct RtcBlock {
    /// Task start.
    pub tasks_start: u32,
    /// Task stop.
    pub tasks_stop: u32,
    /// Task clear.
    pub tasks_clear: u32,
    /// Task trigger overflow.
    pub tasks_trigovrflw: u32,
    _pad0: [u32; 60],
    /// Event tick.
    pub events_tick: u32,
    /// Event overflow.
    pub events_ovrflw: u32,
    _pad1: [u32; 14],
    /// Event compare[0..3].
    pub events_compare: [u32; 4],
    _pad2: [u32; 44],
    /// Interrupt enable set.
    pub intenset: u32,
    /// Interrupt enable clear.
    pub intenclr: u32,
    _pad3: [u32; 13],
    /// Counter value.
    pub counter: u32,
    /// Prescaler.
    pub prescaler: u32,
    _pad4: [u32; 13],
    /// Compare[0..3].
    pub cc: [u32; 4],
}

/// TIMER register block (subset, nRF52832 Product Spec §6.24).
#[repr(C)]
pub struct TimBlock {
    /// Task start.
    pub tasks_start: u32,
    /// Task stop.
    pub tasks_stop: u32,
    /// Task count.
    pub tasks_count: u32,
    /// Task clear.
    pub tasks_clear: u32,
    _pad0: [u32; 60],
    /// Event compare[0..5].
    pub events_compare: [u32; 6],
    _pad1: [u32; 58],
    /// Shortcuts.
    pub shorts: u32,
    _pad2: [u32; 64],
    /// Interrupt enable set.
    pub intenset: u32,
    /// Interrupt enable clear.
    pub intenclr: u32,
    _pad3: [u32; 13],
    /// Mode.
    pub mode: u32,
    /// Bit mode.
    pub bitmode: u32,
    _pad4: u32,
    /// Prescaler.
    pub prescaler: u32,
    _pad5: [u32; 11],
    /// Capture/compare[0..5].
    pub cc: [u32; 6],
}

/// Compile-time base addresses for nRF52832.
pub mod nrf52832 {
    /// GPIO P0
    pub const GPIO_P0: usize = 0x5000_0000;
    /// GPIO P1
    pub const GPIO_P1: usize = 0x5000_0300;

    /// UARTE0
    pub const UARTE0: usize = 0x4000_2000;
    /// UARTE1
    pub const UARTE1: usize = 0x4000_2800;

    /// RTC0
    pub const RTC0: usize = 0x4000_1000;
    /// RTC1
    pub const RTC1: usize = 0x4000_1100;
    /// RTC2
    pub const RTC2: usize = 0x4000_1400;

    /// TIMER0
    pub const TIMER0: usize = 0x4000_8000;
    /// TIMER1
    pub const TIMER1: usize = 0x4000_8100;
    /// TIMER2
    pub const TIMER2: usize = 0x4000_8200;

    /// HFCLK start task.
    pub const CLOCK_HFCLKSTART: usize = 0x4000_0000;
    /// LFCLK start task.
    pub const CLOCK_LFCLKSTART: usize = 0x4000_0008;
}