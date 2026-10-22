//! UART logging backend for RISC-V boards.
//!
//! The RISC-V port has no semihosting channel, so boards install
//! [`UART_LOG`] with [`caplet_log::install_writer`] during boot.  Output
//! goes to the NS16550a UART that the architecture port already
//! initialised in [`crate::boot::riscv_boot`].

use crate::uart::Uart;
use caplet_log::writer::{render_line, LogError, LogWriter};
use caplet_log::Level;

/// [`LogWriter`] that streams the canonical log line to the RISC-V UART.
pub struct UartLogWriter;

impl UartLogWriter {
    /// Create a writer.  It is stateless, so a single static instance is
    /// enough for the whole image.
    pub const fn new() -> Self {
        Self
    }
}

impl Default for UartLogWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl LogWriter for UartLogWriter {
    fn write(&self, level: Level, tag: &str, msg: &str) -> Result<(), LogError> {
        render_line(
            &mut |bytes| {
                for &byte in bytes {
                    Uart::put_char(byte);
                }
                Ok(())
            },
            level,
            tag,
            msg,
        )
    }

    fn flush(&self) -> Result<(), LogError> {
        Ok(())
    }
}

/// Shared writer instance for board boot code.
pub static UART_LOG: UartLogWriter = UartLogWriter::new();
