//! Semihosting log writer
//!
//! Streams each log line through the host's stdout via the
//! `cortex-m-semihosting` crate. Activated by the `semihosting` feature.
//!
//! This is the default backend for the `boards/hello-qemu-cm3` and
//! `boards/hello-qemu-cm4` crates — it works under QEMU with
//! `-semihosting-config enable=on,target=native`.

use crate::level::Level;
use crate::writer::{render_line, LogError, LogWriter};

/// Tag value used to identify the semihosting writer from the
/// installed-writer slot table.
pub const SEMIHOSTING_TAG: u8 = 0xFE;

/// Singleton instance, used by [`crate::install_writer`] internally
/// when the semihosting tag is active. Boards can also install this
/// directly via `caplet_log::install_writer(Some(&caplet_log::semihosting::SEMIHOSTING_INSTANCE))`.
pub static SEMIHOSTING_INSTANCE: SemihostingWriter = SemihostingWriter;

/// `LogWriter` implementation that forwards every line to the QEMU /
/// OpenOCD host stdout via ARM semihosting.
#[derive(Debug, Clone, Copy, Default)]
pub struct SemihostingWriter;

impl LogWriter for SemihostingWriter {
    fn write(&self, level: Level, tag: &str, msg: &str) -> Result<(), LogError> {
        render_line(
            &mut |bytes| {
                // Take the handle freshly on each call: `hstdout()` is
                // documented as not safe to keep around, because the
                // semihosting operation can corrupt the buffer in rare
                // host-gdb edge cases. Allocating a fresh `HStdout` per
                // line is cheap (it is a syscall descriptor, not a
                // buffer) and avoids that class of issue.
                let mut stdout = match cortex_m_semihosting::hio::hstdout() {
                    Ok(s) => s,
                    Err(_) => return Ok(()),
                };
                use core::fmt::Write as _;
                let _ = stdout.write_str(
                    core::str::from_utf8(bytes).unwrap_or("<non-utf8 log line>"),
                );
                Ok(())
            },
            level,
            tag,
            msg,
        )
    }

    fn flush(&self) -> Result<(), LogError> {
        // Semihosting is unbuffered from the perspective of the target:
        // the host sees each write as it happens. Nothing to flush.
        Ok(())
    }
}