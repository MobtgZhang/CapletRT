//! UART log writer
//!
//! Generic [`LogWriter`] implementation over any [`caplet_hal::uart::Uart`]
//! whose `Error` type can be created from the empty `()` (the
//! default for our HAL traits).
//!
//! Use this on real hardware where a serial console is wired up.
//! Pair it with [`crate::install_writer`] at boot:
//!
//! ```ignore
//! static UART: caplet_sync::SpinLock<MyUart> =
//!     caplet_sync::SpinLock::new(MyUart::new());
//! static WRITER: caplet_log::uart::UartWriter<MyUart> =
//!     caplet_log::uart::UartWriter::new(&UART);
//! caplet_log::install_writer(Some(&WRITER));
//! ```

use crate::level::Level;
use crate::writer::{render_line, LogError, LogWriter};
use caplet_hal::uart::Uart;
use caplet_sync::SpinLock;

/// [`LogWriter`] that forwards every line to a [`Uart`].
pub struct UartWriter<U: Uart + 'static> {
    uart: &'static SpinLock<U>,
}

impl<U: Uart + 'static> UartWriter<U> {
    /// Create a new writer wrapping a permanently-installed, lock-protected
    /// UART.  The lock provides the interior mutability required by the
    /// `Uart` trait's `&mut self` methods.
    pub const fn new(uart: &'static SpinLock<U>) -> Self {
        Self { uart }
    }
}

impl<U: Uart + Send> LogWriter for UartWriter<U>
where
    U::Error: From<()>,
{
    fn write(&self, level: Level, tag: &str, msg: &str) -> Result<(), LogError> {
        let mut uart = self.uart.lock();
        render_line(
            &mut |bytes| {
                let s = core::str::from_utf8(bytes).unwrap_or("<non-utf8 log>");
                // Best-effort: swallow the per-byte error.
                let _ = uart.write_all(s.as_bytes());
                Ok(())
            },
            level,
            tag,
            msg,
        )
    }

    fn flush(&self) -> Result<(), LogError> {
        let mut uart = self.uart.lock();
        let _ = uart.flush();
        Ok(())
    }
}
