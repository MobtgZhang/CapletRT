//! Crypto accelerator wrapper.
//!
//! The actual hardware acceleration (AES, SHA, TRNG) is
//! platform-specific; this module only models the *capability*
//! shape: a [`CryptoToken`] must be held to dispatch any of
//! the operations. Without the token the syscall handler
//! rejects the request.

use crate::token::SecurityToken;

/// Errors returned by the crypto syscall dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoError {
    /// Operation denied because the caller has no
    /// [`CryptoToken`] in its CSpace.
    NoToken,
    /// Key length is unsupported for the requested algorithm.
    InvalidKey,
    /// Buffer too small / misaligned for the algorithm.
    BadBuffer,
    /// Hardware reported an error (parity fail, busy timeout…).
    HardwareError,
    /// Algorithm requested is not compiled into this build.
    UnsupportedAlgorithm,
}

/// Tagged crypto operation. Dispatched by the kernel to the
/// appropriate hardware (or software) backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoOp {
    /// AES-128 ECB encrypt
    Aes128EcbEncrypt,
    /// AES-128 ECB decrypt
    Aes128EcbDecrypt,
    /// AES-128 CBC encrypt
    Aes128CbcEncrypt,
    /// AES-128 CBC decrypt
    Aes128CbcDecrypt,
    /// SHA-256 hash
    Sha256,
    /// 128-bit random pull from a hardware TRNG
    Trng128,
    /// True random byte vector of arbitrary length.
    TrngN,
}

/// Marker type used for the [`SecurityToken<Crypto>`] instance.
pub enum Crypto {}

/// Capability-aware crypto handle. Constructed only by the
/// kernel.
pub type CryptoToken = SecurityToken<Crypto>;
