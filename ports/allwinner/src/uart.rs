//! Allwinner A20 (sun7i) UART0 driver.
//!
//! The A20 integrates eight NS16550-compatible UARTs. UART0 is
//! memory-mapped at `0x01C2_8000` and is the console used by both the
//! QEMU `cubieboard` machine and Renode's built-in `a20.repl`.
//!
//! Only polled TX is implemented: that is all the boot/demo path needs
//! and it keeps the driver usable before interrupts or a clock framework
//! have been brought up.

use core::ptr::{read_volatile, write_volatile};

/// Base address of UART0 on the Allwinner A20.
pub const UART0_BASE: usize = 0x01C2_8000;

const RBR_THR: usize = 0x00;
const IER: usize = 0x04;
const FCR: usize = 0x08;
const LCR: usize = 0x0C;
const LSR: usize = 0x14;

const LSR_THRE: u32 = 1 << 5;

/// Polled NS16550 console.
#[derive(Clone, Copy)]
pub struct Ns16550Uart {
    base: usize,
}

impl Ns16550Uart {
    /// Create a driver for the UART mapped at `base`.
    pub const fn new(base: usize) -> Self {
        Self { base }
    }

    /// Create a driver for the A20's UART0.
    pub const fn uart0() -> Self {
        Self::new(UART0_BASE)
    }

    #[inline]
    fn reg(&self, offset: usize) -> *mut u32 {
        (self.base + offset) as *mut u32
    }

    /// Program 115200 8N1 using the 24 MHz default UART clock and
    /// enable the FIFOs.
    pub fn init(&self) {
        // SAFETY: `base` points at a memory-mapped NS16550 register
        // window supplied by the platform (or a test double).
        unsafe {
            write_volatile(self.reg(IER), 0);
            // DLAB=1, 8 data bits, no parity, one stop bit.
            write_volatile(self.reg(LCR), 0x83);
            // 24 MHz / (16 * 115200) = 13.02.
            write_volatile(self.reg(RBR_THR), 13); // DLL
            write_volatile(self.reg(IER), 0); // DLM (DLAB still set)
            write_volatile(self.reg(LCR), 0x03);
            // Enable and clear both FIFOs, 14-byte trigger.
            write_volatile(self.reg(FCR), 0xC7);
        }
    }

    /// Block until the transmit holding register is empty, then queue
    /// one byte.
    pub fn putc(&self, byte: u8) {
        // SAFETY: see `init`.
        unsafe {
            while read_volatile(self.reg(LSR)) & LSR_THRE == 0 {
                core::hint::spin_loop();
            }
            write_volatile(self.reg(RBR_THR), u32::from(byte));
        }
    }

    /// Write a string, translating `\n` to CRLF for serial terminals.
    pub fn puts(&self, s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.putc(b'\r');
            }
            self.putc(byte);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uart0_is_page_aligned() {
        assert_eq!(UART0_BASE & 0xFFF, 0);
    }

    #[test]
    fn register_offsets_match_ns16550() {
        assert_eq!(RBR_THR, 0x00);
        assert_eq!(IER, 0x04);
        assert_eq!(FCR, 0x08);
        assert_eq!(LCR, 0x0C);
        assert_eq!(LSR, 0x14);
        assert_eq!(LSR_THRE, 0x20);
    }
}
