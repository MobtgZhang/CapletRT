//! Capability-aware wrappers around security-critical
//! peripherals (cryptographic accelerators, internal flash
//! controllers, JTAG / debug-lock switches).
//!
//! Each wrapper is a *token* that proves the holder has been
//! granted the corresponding capability. Tokens are
//! non-revocable by the holder — only the kernel can revoke
//! them by clearing the slot in the capability table.

#![no_std]
#![deny(missing_docs)]

pub mod crypto;
pub mod flash;
pub mod jtag;
pub mod token;

pub use crypto::{CryptoError, CryptoOp, CryptoToken};
pub use flash::{FlashError, FlashToken, FlashRegion};
pub use jtag::{JtagError, JtagLock, JtagState};
pub use token::{CNonce, SecurityToken};