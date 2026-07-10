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
//! static UART: Mutex<MyUart> = Mutex::new(MyUart::new());
//! caplet_log::install_writer(Some(&caplet_log::uart::UartWriter::new(&UART)));
//! ```

use crate::level::Level;
use crate::writer::{render_line, LogError, LogWriter};
use caplet_hal::uart::Uart;
use core::fmt::Write as _;

/// [`LogWriter`] that forwards every line to a [`Uart`].
pub struct UartWriter<U: Uart> {
    uart: *const U,
}

// SAFETY: the wrapped `Uart` is expected to be a `'static` instance
// protected by a Mutex (typically). We only ever call `&self`
// methods through the trait.
unsafe impl<U: Uart + Send + Sync> Send for UartWriter<U> {}
unsafe impl<U: Uart + Send + Sync> Sync for UartWriter<U> {}

impl<U: Uart> UartWriter<U> {
    /// Create a new writer wrapping the given UART. The pointer must
    /// outlive the program (i.e. the underlying UART is a `'static`).
    pub fn new(uart: &'static U) -> Self {
        Self { uart: uart as *const U }
    }
}

impl<U: Uart> LogWriter for UartWriter<U>
where
    U::Error: From<()>,
{
    fn write(&self, level: Level, tag: &str, msg: &str) -> Result<(), LogError> {
        // SAFETY: see the `Send`/`Sync` impls — caller guarantees
        // the UART is a `'static` and concurrency-safe.
        let uart = unsafe { &*self.uart };
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
        let uart = unsafe { &*self.uart };
        let _ = uart.flush();
        Ok(())
    }
}