//! UART driver for QEMU virt (NS16550a-compatible).
//!
//! UART0 on QEMU virt is a standard NS16550a at 0x1000_0000.
//! Registers (8B each, accessed as 32-bit words at offsets):
//!
//!   Offset  0: RHR / THR (receive / transmit holding register)
//!   Offset  1: IER (interrupt enable register)
//!   Offset  2: IIR (interrupt identification / FIFO control)
//!   Offset  3: LCR (line control register)
//!   Offset  4: MCR (modem control register)
//!   Offset  5: LSR (line status register)
//!   Offset  6: MSR (modem status register)
//!   Offset  7: SCR (scratch register)
//!
//! QEMU virt UART is configured as:
//!   115200 baud, 8N1, no interrupts (polling mode).

/// UART0 MMIO base on QEMU virt.
const UART_BASE: usize = 0x1000_0000;

/// Register offsets (8-bit access, but QEMU maps them as 32-bit).
const REG_RHR: usize = 0x0;  // Receive / Transmit holding register.
const REG_IER: usize = 0x1;  // Interrupt enable.
const REG_LSR: usize = 0x5;  // Line status register.
const REG_FCR: usize = 0x2;  // FIFO control register.
const REG_LCR: usize = 0x3;  // Line control register.

/// LSR bits.
const LSR_TX_IDLE: u8 = 1 << 5;  // Transmitter empty.
const LSR_RX_READY: u8 = 1 << 0;  // Receive data ready.

/// The UART device.
pub struct Uart;

impl Uart {
    /// Initialize the UART (115200 8N1, polling mode).
    ///
    /// On QEMU virt the UART is already configured by the firmware,
    /// so this mainly enables transmitter/receiver.
    pub fn init() {
        // Configure: 115200 baud, 8 data bits, 1 stop, no parity.
        // Divisor = CLINT_HZ / (16 × 115200) = 50_000_000 / 1_843_200 ≈ 27.
        let divisor = 27u16;
        // LCR = 0x80 → set DLAB to access divisor latch.
        Self::write8(REG_LCR, 0x80);
        // Write divisor LSB and MSB.
        Self::write8(0, (divisor & 0xFF) as u8);
        Self::write8(1, (divisor >> 8) as u8);
        // LCR = 0x03 → 8N1, DLAB = 0.
        Self::write8(REG_LCR, 0x03);
        // FCR = 0x01 → enable FIFO (optional).
        Self::write8(REG_FCR, 0x01);
        // MCR = 0x03 → DTR + RTS.
        Self::write8(4, 0x03);
        // IER = 0 → polling mode (no interrupts).
        Self::write8(REG_IER, 0x00);
    }

    /// Write a byte to the UART transmit register.
    ///
    /// Blocks until the transmitter is idle.
    pub fn put_char(c: u8) {
        // Wait for LSR TX_IDLE.
        while Self::read8(REG_LSR) & LSR_TX_IDLE == 0 {}
        Self::write8(REG_RHR, c);
    }

    /// Write a string (blocking).
    pub fn put_str(s: &str) {
        for &b in s.as_bytes() {
            Self::put_char(b);
        }
    }

    /// Write a string with newline (blocking).
    pub fn put_line(s: &str) {
        Self::put_str(s);
        Self::put_char(b'\n');
    }

    /// Read a byte from the UART receive register.
    ///
    /// Returns `None` if no data is available.
    pub fn get_char() -> Option<u8> {
        if Self::read8(REG_LSR) & LSR_RX_READY != 0 {
            Some(Self::read8(REG_RHR))
        } else {
            None
        }
    }

    /// Handle a UART interrupt (called from PLIC).
    ///
    /// Reads all available bytes from the receive FIFO.
    /// In a full implementation this would push to a ring buffer.
    pub fn handle_irq() {
        while let Some(c) = Self::get_char() {
            // Echo back in debug mode.
            #[cfg(feature = "echo_uart")]
            Self::put_char(c);
        }
    }

    /// Check if the UART has a character available.
    pub fn has_char() -> bool {
        Self::read8(REG_LSR) & LSR_RX_READY != 0
    }

    // ── MMIO helpers ────────────────────────────────────────────────

    fn read8(reg: usize) -> u8 {
        let raw = (UART_BASE + reg) as *mut u8;
        // SAFETY: addr is a known UART MMIO register.
        unsafe { core::ptr::read_volatile(raw) }
    }

    fn write8(reg: usize, val: u8) {
        let raw = (UART_BASE + reg) as *mut u8;
        // SAFETY: addr is a known UART MMIO register.
        unsafe { core::ptr::write_volatile(raw, val) };
    }
}
