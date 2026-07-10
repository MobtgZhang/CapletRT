//! JTAG / debug-lock state.
//!
//! On STM32F4 and nRF52 the JTAG / SWD debug port can be
//! permanently disabled by writing into option bytes. The
//! [`JtagLock`] capability encodes the request "disable JTAG /
//! SWD / trace from now on" — once used, the capability is
//! consumed and cannot be re-issued until a full reset.

use crate::token::SecurityToken;

use core::sync::atomic::{AtomicU8, Ordering};

/// Errors returned by the JTAG capability API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JtagError {
    /// Caller has no [`JtagLock`] token.
    NoToken,
    /// JTAG / SWD was already disabled; the lock is one-shot.
    AlreadyLocked,
    /// Hardware refused (option-byte write protected).
    HardwareError,
}

/// State of the debug port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JtagState {
    /// Debug port is enabled.
    Enabled,
    /// Debug port is disabled.
    Disabled,
}

// One global state machine; the lock is per-process at the
// hardware level so an `AtomicU8` is enough.
static STATE: AtomicU8 = AtomicU8::new(JtagState::Enabled as u8);

/// Marker for [`SecurityToken<Jtag>`].
pub enum Jtag {}

/// Token type for the JTAG lock operation.
pub type JtagLock = SecurityToken<Jtag>;

/// Request the kernel to permanently disable JTAG / SWD /
///
/// # Errors
/// Returns [`JtagError::AlreadyLocked`] if it has already been
/// disabled, or [`JtagError::NoToken`] if no [`JtagLock`] is
/// present in the calling CSpace.
pub fn disable(_token: &JtagLock) -> Result<(), JtagError> {
    // Pretend we own the kernel; in real life the syscall
    // handler validates the token first.
    match STATE.load(Ordering::Acquire) {
        x if x == JtagState::Disabled as u8 => Err(JtagError::AlreadyLocked),
        _ => {
            STATE.store(JtagState::Disabled as u8, Ordering::Release);
            Ok(())
        }
    }
}

/// Read the current debug-port state. Cheap, callable from any
/// context.
pub fn state() -> JtagState {
    match STATE.load(Ordering::Acquire) {
        x if x == JtagState::Disabled as u8 => JtagState::Disabled,
        _ => JtagState::Enabled,
    }
}

/// Reset the lock state to "enabled". Available only in
/// `debug` builds; production builds compile it to a no-op.
///
/// # Safety
/// Intended for unit tests only.
#[cfg(any(test, feature = "test-reset"))]
#[allow(unexpected_cfgs)]
pub unsafe fn reset_for_testing() {
    STATE.store(JtagState::Enabled as u8, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_is_one_shot() {
        unsafe { reset_for_testing() };
        let t = unsafe { SecurityToken::<Jtag>::new() };
        // SAFETY: in a single-threaded test we can issue a token directly.
        assert!(disable(&t).is_ok());
        assert_eq!(state(), JtagState::Disabled);
        let t2 = unsafe { SecurityToken::<Jtag>::new() };
        assert_eq!(disable(&t2), Err(JtagError::AlreadyLocked));
    }
}