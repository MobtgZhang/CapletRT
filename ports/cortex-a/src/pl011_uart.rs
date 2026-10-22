//! PL011 UART driver for the vexpress-a9 board.
//!
//! Minimal polling implementation sufficient for `print!`-style
//! debug output during M3 bring-up.

use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicPtr, Ordering};

/// Base address of the UART (from `boot.rs`).
pub const PL011_BASE: usize = 0x1000_9000;

/// Data register (write = TX, read = RX).
const UART_DR: usize = PL011_BASE;
/// Flag register.
const UART_FR: usize = PL011_BASE + 0x18;
/// Integer baud-rate divisor.
#[cfg(target_arch = "arm")]
const UART_IBRD: usize = PL011_BASE + 0x24;
/// Fractional baud-rate divisor.
#[cfg(target_arch = "arm")]
const UART_FBRD: usize = PL011_BASE + 0x28;
/// Line control register.
#[cfg(target_arch = "arm")]
const UART_LCRH: usize = PL011_BASE + 0x2C;
/// Control register.
#[cfg(target_arch = "arm")]
const UART_CR: usize = PL011_BASE + 0x30;

/// TX FIFO full bit in `UART_FR`.
const FR_TXFF: u32 = 1 << 5;

/// Static instance used by the panic handler.
static UART_INST: AtomicPtr<Pl011Uart> = AtomicPtr::new(core::ptr::null_mut());

/// PL011 UART in polling mode.
pub struct Pl011Uart {
    base: usize,
}

impl Pl011Uart {
    /// Construct a new driver for the address at `base`.
    pub const fn new(base: usize) -> Self {
        Self { base }
    }

    /// Set this instance as the panic-handler target.
    pub fn install(&self) {
        let p: *const Self = self;
        UART_INST.store(p as *mut Self, Ordering::Release);
    }

    /// Take the previously installed instance, if any.
    pub fn take() -> Option<Pl011Uart> {
        let p = UART_INST.load(Ordering::Acquire);
        if p.is_null() {
            None
        } else {
            // SAFETY: `install` only stores pointers to `Pl011Uart`
            // values that remain alive for the program lifetime
            // (normally a `static`).  Copy the address out rather than
            // aliasing the installed object.
            let base = unsafe { (*p).base };
            if base == 0 {
                None
            } else {
                Some(Self { base })
            }
        }
    }

    /// Initialise the UART for 115_200 8N1.
    pub fn init(&mut self) {
        // On non-ARM targets there is no PL011 to talk to; the
        // caller can still install a no-op stub via `take()`.
        #[cfg(not(target_arch = "arm"))]
        {
            // No PL011 exists on the host; the driver stays a no-op stub.
        }
        #[cfg(target_arch = "arm")]
        unsafe {
            // Disable UART.
            write_volatile((self.base + UART_CR - PL011_BASE) as *mut u32, 0);
            // Set baud rate (24 MHz reference, 115_200 baud -> divisor 13).
            write_volatile((self.base + UART_IBRD - PL011_BASE) as *mut u32, 13);
            write_volatile((self.base + UART_FBRD - PL011_BASE) as *mut u32, 0x04);
            // 8N1, FIFO enabled.
            write_volatile((self.base + UART_LCRH - PL011_BASE) as *mut u32, 0x70);
            // Enable UART + TX + RX.
            write_volatile((self.base + UART_CR - PL011_BASE) as *mut u32, 0x301);
        }
    }

    /// Write a single byte.
    pub fn putc(&self, c: u8) {
        unsafe {
            while (read_volatile((self.base + UART_FR - PL011_BASE) as *const u32) & FR_TXFF) != 0 {
                core::hint::spin_loop();
            }
            write_volatile((self.base + UART_DR - PL011_BASE) as *mut u32, c as u32);
        }
    }

    /// Write a string.
    pub fn puts(&self, s: &str) {
        for c in s.bytes() {
            if c == b'\n' {
                self.putc(b'\r');
            }
            self.putc(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_address_is_page_aligned() {
        assert_eq!(PL011_BASE & 0xFFF, 0);
    }

    #[test]
    fn take_returns_some_after_install() {
        let mut u = Pl011Uart::new(PL011_BASE);
        u.init();
        u.install();
        // The installed pointer must round-trip: the previous
        // implementation returned a driver with `base = 0` and would
        // fault on the first register access on real hardware.
        let taken = Pl011Uart::take().expect("installed UART");
        assert_eq!(taken.base, PL011_BASE);
    }
}
