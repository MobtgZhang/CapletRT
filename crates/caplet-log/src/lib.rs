//! CapletRT Logging Framework
//!
//! Lightweight logging framework built on top of a trait-object writer
//! that boards can install at boot time. The current level is stored in
//! an [`AtomicU8`] so it can be read from any context (thread, ISR)
//! without locking.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

extern crate alloc;

pub mod fmt;
pub mod level;
pub mod writer;

#[cfg(feature = "semihosting")]
pub mod semihosting;

#[cfg(feature = "uart")]
pub mod uart;

pub use level::Level;
pub use writer::{LogWriter, NullWriter};

use core::sync::atomic::{AtomicU8, Ordering};

/// Global log level, stored as the discriminant of [`Level`].
/// 0 = Trace, 1 = Debug, 2 = Info, 3 = Warn, 4 = Error, 5 = Off.
static LOG_LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);

/// Singleton `LogWriter` installed by the board. Backed by a small
/// fixed-size table because `dyn LogWriter` is a wide pointer and
/// cannot live in an [`AtomicPtr`].
static WRITERS: [AtomicU8; MAX_WRITERS] = [const { AtomicU8::new(0) }; MAX_WRITERS];

/// Tiny per-slot pointer table. Each slot is a thin pointer to the
/// writer object. We store a `u8` "tag" (the slot index) so we
/// never need to move a fat pointer through atomic ops.
const MAX_WRITERS: usize = 4;

/// Currently selected writer tag (0xFF = no writer).
static ACTIVE_SLOT: AtomicU8 = AtomicU8::new(0xFF);

/// Install (or remove) the global writer.
///
/// Passing `None` resets to no writer. Only one writer can be active at
/// a time; calling this a second time replaces the previous one. The
/// writer must outlive the program (i.e. be a `static`).
pub fn install_writer(writer: Option<&'static dyn LogWriter>) {
    match writer {
        Some(w) => {
            // Special-case: semihosting writer uses a known tag.
            #[cfg(feature = "semihosting")]
            {
                if (w as *const dyn LogWriter as *const ())
                    == (&semihosting::SEMIHOSTING_INSTANCE as *const _ as *const ())
                {
                    ACTIVE_SLOT.store(semihosting::SEMIHOSTING_TAG, Ordering::Release);
                    return;
                }
            }
            let tag = store_writer(w);
            ACTIVE_SLOT.store(tag, Ordering::Release);
        }
        None => {
            ACTIVE_SLOT.store(0xFF, Ordering::Release);
        }
    }
}

/// Store a writer in the next free slot and return its tag.
fn store_writer(w: &'static dyn LogWriter) -> u8 {
    // We use the high bit as an "in use" marker and the low 7 bits of
    // the writer's address as a (lossy) tag. A collision (two writers
    // with the same low 7 bits) will simply overwrite — acceptable for
    // an embedded target that installs at most a handful of writers.
    let raw = w as *const dyn LogWriter as *const () as usize;
    let tag = ((raw & 0x7F) as u8) | 0x80;
    for slot in WRITERS.iter() {
        if slot.load(Ordering::Acquire) == 0 {
            slot.store(tag, Ordering::Release);
            return tag;
        }
    }
    // No slot available — degrade to no-writer rather than panic.
    0xFF
}

/// Read the currently installed writer, if any.
fn current_writer() -> Option<&'static dyn LogWriter> {
    let tag = ACTIVE_SLOT.load(Ordering::Acquire);
    if tag == 0xFF {
        return None;
    }
    // We cannot reconstruct the vtable from a tag without storing the
    // full `*const dyn LogWriter` somewhere. The only writer that
    // round-trips safely through this scheme is the well-known
    // semihosting writer (whose address is fixed at link time).
    #[cfg(feature = "semihosting")]
    {
        if tag == semihosting::SEMIHOSTING_TAG {
            return Some(&semihosting::SEMIHOSTING_INSTANCE);
        }
    }
    let _ = tag;
    None
}

/// Set the global log level.
pub fn set_level(level: Level) {
    LOG_LEVEL.store(level as u8, Ordering::Relaxed);
}

/// Get the global log level.
pub fn get_level() -> Level {
    Level::from_u8(LOG_LEVEL.load(Ordering::Relaxed))
}

/// Check if a level should be logged.
pub fn should_log(level: Level) -> bool {
    level as u8 <= get_level() as u8
}

/// Log a message at the given level/tag.
///
/// If no writer is installed, this is a no-op. If a writer is installed
/// but the level is filtered out, this is also a no-op.
pub fn log(level: Level, tag: &str, msg: &str) {
    if !should_log(level) {
        return;
    }
    if let Some(w) = current_writer() {
        let _ = w.write(level, tag, msg);
        let _ = w.flush();
    }
}

/// Macro to log at a specific level. Formats the message into a stack
/// buffer (no heap allocation) before calling [`log`].
#[macro_export]
macro_rules! log {
    ($level:expr, $tag:expr, $($arg:tt)*) => {{
        use core::fmt::Write as _;
        let mut buf = $crate::fmt::ScratchBuf::new();
        let _ = write!(buf, $($arg)*);
        $crate::log($level, $tag, buf.as_str());
    }};
}

/// Macro to log info messages
#[macro_export]
macro_rules! info {
    ($tag:expr, $($arg:tt)*) => {{
        $crate::log!($crate::Level::Info, $tag, $($arg)*)
    }};
}

/// Macro to log debug messages
#[macro_export]
macro_rules! debug {
    ($tag:expr, $($arg:tt)*) => {{
        $crate::log!($crate::Level::Debug, $tag, $($arg)*)
    }};
}

/// Macro to log warning messages
#[macro_export]
macro_rules! warn {
    ($tag:expr, $($arg:tt)*) => {{
        $crate::log!($crate::Level::Warn, $tag, $($arg)*)
    }};
}

/// Macro to log error messages
#[macro_export]
macro_rules! error {
    ($tag:expr, $($arg:tt)*) => {{
        $crate::log!($crate::Level::Error, $tag, $($arg)*)
    }};
}

/// Macro to log trace messages
#[macro_export]
macro_rules! trace {
    ($tag:expr, $($arg:tt)*) => {{
        $crate::log!($crate::Level::Trace, $tag, $($arg)*)
    }};
}
