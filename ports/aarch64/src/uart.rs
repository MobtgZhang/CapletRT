//! PL011 UART driver used by QEMU virt and Renode Cortex-A53.

/// PL011 base address on QEMU virt and the Renode Cortex-A53 platform.
pub const UART_BASE: usize = 0x0900_0000;

const UART_DR: usize = 0x00;
const UART_FR: usize = 0x18;
const UART_IBRD: usize = 0x24;
const UART_FBRD: usize = 0x28;
const UART_LCRH: usize = 0x2C;
const UART_CR: usize = 0x30;

const FR_TXFF: u32 = 1 << 5;

/// PL011 polling UART.
pub struct Uart;

impl Uart {
    /// Initialise 115200 8N1 with FIFOs enabled.
    pub fn init() {
        // SAFETY: all addresses are fixed MMIO registers of the PL011.
        unsafe {
            core::ptr::write_volatile((UART_BASE + UART_CR) as *mut u32, 0);
            core::ptr::write_volatile((UART_BASE + UART_IBRD) as *mut u32, 13);
            core::ptr::write_volatile((UART_BASE + UART_FBRD) as *mut u32, 1);
            core::ptr::write_volatile((UART_BASE + UART_LCRH) as *mut u32, 0x70);
            core::ptr::write_volatile((UART_BASE + UART_CR) as *mut u32, 0x301);
        }
    }

    /// Write one byte, blocking while the TX FIFO is full.
    pub fn put_char(byte: u8) {
        // SAFETY: fixed PL011 MMIO registers.
        unsafe {
            while core::ptr::read_volatile((UART_BASE + UART_FR) as *const u32) & FR_TXFF != 0 {
                core::hint::spin_loop();
            }
            core::ptr::write_volatile((UART_BASE + UART_DR) as *mut u32, byte as u32);
        }
    }

    /// Write a string, translating LF to CRLF.
    pub fn put_str(s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                Self::put_char(b'\r');
            }
            Self::put_char(byte);
        }
    }

    /// Write a string followed by a newline.
    pub fn put_line(s: &str) {
        Self::put_str(s);
        Self::put_char(b'\n');
    }

    /// Print a value as lowercase hexadecimal.
    pub fn put_hex(mut value: u64) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut buf = [b'0'; 16];
        let mut i = 16;
        loop {
            i -= 1;
            buf[i] = HEX[(value & 0xf) as usize];
            value >>= 4;
            if value == 0 {
                break;
            }
        }
        for &b in &buf[i..] {
            Self::put_char(b);
        }
    }
}
