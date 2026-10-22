//! QEMU UART driver

use caplet_hal::uart::{Uart, UartConfig};

/// UART base address for QEMU (ARM UART)
pub const UART0_BASE: u32 = 0x4000_4000;

/// UART registers
#[derive(Debug)]
#[repr(C)]
pub struct UartRegisters {
    /// UARTDR - Data Register
    pub dr: u32,
    /// UARTECR - Receive Status / Error Clear Register
    pub ecr: u32,
    /// Reserved
    _reserved: [u32; 4],
    /// UARTFR - Flag Register
    pub fr: u32,
}

/// UART flag bits
mod flags {
    pub const TXFE: u32 = 1 << 7; // TX FIFO empty
    pub const RXFE: u32 = 1 << 4; // RX FIFO empty
    pub const BUSY: u32 = 1 << 3; // UART busy
}

/// QEMU UART device
pub struct QemuUart {
    base: u32,
}

impl QemuUart {
    /// Create a new QEMU UART
    pub fn new() -> Self {
        Self { base: UART0_BASE }
    }

    /// Create with custom base address
    pub fn with_base(base: u32) -> Self {
        Self { base }
    }

    /// Read a register
    fn read_reg(&self, offset: u32) -> u32 {
        // In actual implementation, this would use volatile read
        unsafe { core::ptr::read_volatile((self.base + offset) as *const u32) }
    }

    /// Write a register
    fn write_reg(&self, offset: u32, value: u32) {
        // In actual implementation, this would use volatile write
        unsafe {
            core::ptr::write_volatile((self.base + offset) as *mut u32, value);
        }
    }

    /// Check if TX is ready
    fn tx_ready(&self) -> bool {
        (self.read_reg(0x18) & flags::TXFE) == 0
    }

    /// Check if RX has data
    fn rx_ready(&self) -> bool {
        (self.read_reg(0x18) & flags::RXFE) == 0
    }
}

impl Default for QemuUart {
    fn default() -> Self {
        Self::new()
    }
}

impl Uart for QemuUart {
    type Error = QemuUartError;

    fn configure(&mut self, _config: UartConfig) -> Result<(), Self::Error> {
        // QEMU UART doesn't need configuration for basic use
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> Result<usize, Self::Error> {
        let mut written = 0;
        for &byte in data {
            // Wait for TX to be ready
            while !self.tx_ready() {}
            self.write_reg(0x00, byte as u32);
            written += 1;
        }
        Ok(written)
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        let mut read = 0;
        for byte in buffer {
            // Wait for RX data
            while !self.rx_ready() {}
            *byte = self.read_reg(0x00) as u8;
            read += 1;
        }
        Ok(read)
    }

    fn available(&self) -> bool {
        self.rx_ready()
    }

    fn tx_empty(&self) -> bool {
        (self.read_reg(0x18) & (flags::TXFE | flags::BUSY)) == flags::TXFE
    }
}

/// QEMU UART error type
#[derive(Debug)]
pub enum QemuUartError {
    /// Not configured
    NotConfigured,
    /// Timeout
    Timeout,
}

impl core::fmt::Display for QemuUartError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "UART not configured"),
            Self::Timeout => write!(f, "UART operation timed out"),
        }
    }
}

/// Write a string to UART
pub fn uart_write(s: &str) {
    let mut uart = QemuUart::new();
    let _ = uart.write(s.as_bytes());
}

/// Write a string with newline
pub fn uart_write_line(s: &str) {
    uart_write(s);
    uart_write("\r\n");
}
