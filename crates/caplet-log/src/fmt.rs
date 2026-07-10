//! Formatting utilities for logging
//!
//! Provides a fixed-size scratch buffer that implements
//! [`core::fmt::Write`] so the [`log!`](../macro.log.html) macro family
//! can format into stack memory without heap allocation.

use core::fmt;

/// Stack-allocated, fixed-size scratch buffer.
///
/// Used by the `log!` macros to render user-supplied format arguments
/// into a bounded region of memory. Writes past [`Self::CAP`] are
/// silently truncated, which is acceptable for logging.
///
/// The default capacity is 192 bytes — large enough for the longest
/// expected log line on an embedded target (e.g. `[LEVEL] tag: 32-byte
/// UUID / 64-bit hex / text`).
pub struct ScratchBuf {
    buf: [u8; Self::CAP],
    len: usize,
}

impl ScratchBuf {
    /// Default scratch buffer capacity, in bytes.
    pub const CAP: usize = 192;

    /// Create an empty scratch buffer.
    pub const fn new() -> Self {
        Self {
            buf: [0u8; Self::CAP],
            len: 0,
        }
    }

    /// Number of bytes currently written.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True if no bytes have been written.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Borrow the formatted bytes as a `&str`.
    ///
    /// The contents are not guaranteed to be valid UTF-8 (the writer
    /// silently truncates), so this returns `&str` only when the
    /// underlying bytes happen to be valid UTF-8. To be safe the
    /// logging macros round-trip through this method; callers using
    /// non-ASCII arguments should use the raw `as_bytes()` view.
    pub fn as_str(&self) -> &str {
        // Best-effort: assume ASCII. `core::str::from_utf8` is not
        // `const`-friendly across older Rust versions, so use the
        // fallible `from_utf8` and substitute a placeholder on error.
        match core::str::from_utf8(&self.buf[..self.len]) {
            Ok(s) => s,
            Err(_) => core::str::from_utf8(b"<log buffer contains non-utf8>")
                .unwrap_or(""),
        }
    }

    /// View the raw bytes written so far.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl Default for ScratchBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Write for ScratchBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let remaining = Self::CAP - self.len;
        let to_copy = core::cmp::min(s.len(), remaining);
        self.buf[self.len..self.len + to_copy]
            .copy_from_slice(&s.as_bytes()[..to_copy]);
        self.len += to_copy;
        // Always succeed — overflow is silent truncation.
        Ok(())
    }
}

/// Format a pre-formed log line into a buffer with the legacy
/// `[LEVEL] tag: msg\n` shape.
///
/// This helper predates [`ScratchBuf`] and is kept for backward
/// compatibility with any user that calls it directly.
pub fn format_into(buf: &mut [u8], level: &str, tag: &str, msg: &str) -> usize {
    let mut pos = 0;

    // Format: [LEVEL] tag: msg\n

    if pos < buf.len() {
        buf[pos] = b'[';
        pos += 1;
    }

    for &b in level.as_bytes() {
        if pos < buf.len() {
            buf[pos] = b;
            pos += 1;
        }
    }

    if pos + 3 < buf.len() {
        buf[pos] = b']';
        buf[pos + 1] = b' ';
        pos += 2;
    }

    for &b in tag.as_bytes() {
        if pos < buf.len() {
            buf[pos] = b;
            pos += 1;
        }
    }

    if pos + 2 < buf.len() {
        buf[pos] = b':';
        buf[pos + 1] = b' ';
        pos += 2;
    }

    for &b in msg.as_bytes() {
        if pos < buf.len() {
            buf[pos] = b;
            pos += 1;
        }
    }

    if pos < buf.len() {
        buf[pos] = b'\n';
        pos += 1;
    }

    pos
}
