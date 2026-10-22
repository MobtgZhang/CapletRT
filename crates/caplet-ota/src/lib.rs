//! CapletRT OTA Update Manager
//!
//! This crate provides:
//!   - [`BootManager`] — A/B slot selection and boot flag management.
//!   - [`Bootloader`]  — secure boot selection and rollback.
//!   - [`Updater`]     — firmware download, verification, and atomic swap.
//!   - [`Verifier`]    — SHA-256 + Ed25519 signature verification.
//!
//! It is the kernel-side component for the M4 OTA system.
//! The companion bootloader is in [`bootloader.rs`] and runs in the
//! ROM-resident first-stage boot sector (outside the OTA partition).

// MSRV 1.75 predates `is_multiple_of`; the modulo form is intentional.
#![allow(clippy::manual_is_multiple_of)]

pub mod boot_manager;
pub mod bootloader;
pub mod downloader;
pub mod flash;
pub mod rollback;
pub mod verifier;

pub use boot_manager::{BootManager, BootRecord, BootSlot, BootState};
pub use bootloader::{BootFailure, BootOutcome, BootSelection, Bootloader};
pub use downloader::{
    preflight, DownloadError, FirmwareSource, SliceSource, UpdateResult, Updater,
};
pub use flash::{FlashIoError, FlashStorage, RamFlash};
pub use rollback::RollbackGuard;
pub use verifier::{FirmwareHeader, TrustStore, TrustedKey, Verifier, VerifyError};
