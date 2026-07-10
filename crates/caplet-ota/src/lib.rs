//! CapletRT OTA Update Manager
//!
//! This crate provides:
//!   - [`BootManager`] — A/B slot selection and boot flag management.
//!   - [`Updater`]     — firmware download, verification, and atomic swap.
//!   - [`Verifier`]    — SHA-256 + Ed25519 signature verification.
//!
//! It is the kernel-side component for the M4 OTA system.
//! The companion bootloader is in [`bootloader.rs`] and runs in the
//! ROM-resident first-stage boot sector (outside the OTA partition).

pub mod boot_manager;
pub mod verifier;
pub mod rollback;

pub use boot_manager::{BootManager, BootSlot, BootState};
pub use verifier::Verifier;
pub use rollback::RollbackGuard;
