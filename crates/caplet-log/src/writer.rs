//! Log writer trait
//!
//! Backends (semihosting, UART, RTT, …) implement this trait and are
//! installed globally via [`crate::install_writer`].

use crate::fmt::ScratchBuf;
use crate::level::Level;
use core::fmt::Write as _;

/// Log writer trait
///
/// Implementations must be safe to call from any context (thread mode
/// and interrupt) and must use interior mutability (e.g. an [`Atomic*`]
/// or a `SpinLock`) for any state that can be observed across calls.
/// The kernel and boards install a single `'static` writer via
/// [`crate::install_writer`]; the writer is invoked through a shared
/// reference so the trait methods take `&self`.
pub trait LogWriter: Sync {
    /// Write a log message
    fn write(&self, level: Level, tag: &str, msg: &str) -> Result<(), LogError>;

    /// Flush any buffered output
    fn flush(&self) -> Result<(), LogError>;
}

/// A no-op writer. Useful as a placeholder or in tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullWriter;

impl LogWriter for NullWriter {
    fn write(&self, _level: Level, _tag: &str, _msg: &str) -> Result<(), LogError> {
        Ok(())
    }
    fn flush(&self) -> Result<(), LogError> {
        Ok(())
    }
}

/// Log error type
#[derive(Debug)]
pub struct LogError {
    _priv: (),
}

impl LogError {
    /// Create a new log error
    pub fn new() -> Self {
        Self { _priv: () }
    }
}

impl Default for LogError {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Display for LogError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Log error")
    }
}

/// Helper used by simple `LogWriter` implementations that just need to
/// assemble the canonical `[LEVEL] tag: msg\n` line and stream it
/// byte-by-byte to a `core::fmt::Write`-style sink.
#[allow(dead_code)]
pub fn render_line(
    sink: &mut dyn FnMut(&[u8]) -> Result<(), LogError>,
    level: Level,
    tag: &str,
    msg: &str,
) -> Result<(), LogError> {
    let mut buf = ScratchBuf::new();
    let _ = writeln!(buf, "[{}] {}: {}", level.name(), tag, msg);
    sink(buf.as_bytes())
}
