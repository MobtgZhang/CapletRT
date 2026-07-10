//! FAT32 placeholder backend.
//!
//! The real FAT32 driver will come from a fork of
//! `embedded-fatfs` in M2.x. Until then the placeholder keeps
//! the same shape so a board can mount either LittleFS or FAT32
//! depending on the storage medium.
//!
//! ## Implementation Plan (M2.x)
//!
//! To implement the real FAT32 driver:
//!
//! 1. Add dependency:
//!    ```toml
//!    # In Cargo.toml of caplet-fs
//!    fatfs = { git = "https://github.com/rust-embedded-community/embedded-fatfs" }
//!    ```
//!
//! 2. Implement the `embedded_io::Blocking` trait for the block device
//!
//! 3. Create a wrapper struct:
//!    ```ignore
//!    pub struct Fat32Driver<S: BlockDevice> {
//!        fs: FatFs<S>,
//!        partition: Partition,
//!    }
//!    ```
//!
//! 4. Implement `FsDriver` trait by delegating to FatFs methods
//!
//! ## Usage
//!
//! ```ignore
//! use caplet_fs::fat32::Fat32Driver;
//! use embedded_io::asynch::BlockDevice;
//!
//! let block_dev = /* your SD card / flash block device */;
//! let fat = Fat32Driver::new(block_dev)?;
//! mount_table::mount("/sd", fat);
//! ```

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::driver::{DirEntry, DriverError, FileMode, FsDriver, SeekFrom};

/// Stub FAT32 driver.
///
/// This is a placeholder that returns `Unsupported` errors.
/// Full implementation planned for M2.x phase.
pub struct Fat32Placeholder;

impl Fat32Placeholder {
    /// Wrap in an Arc.
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl FsDriver for Fat32Placeholder {
    fn open(&self, _p: &str, _m: FileMode) -> Result<u32, DriverError> {
        Err(DriverError::Unsupported)
    }
    fn close(&self, _h: u32) -> Result<(), DriverError> {
        Err(DriverError::Unsupported)
    }
    fn read(&self, _h: u32, _b: &mut [u8]) -> Result<usize, DriverError> {
        Err(DriverError::Unsupported)
    }
    fn write(&self, _h: u32, _b: &[u8]) -> Result<usize, DriverError> {
        Err(DriverError::Unsupported)
    }
    fn seek(&self, _h: u32, _f: SeekFrom) -> Result<u64, DriverError> {
        Err(DriverError::Unsupported)
    }
    fn flush(&self, _h: u32) -> Result<(), DriverError> {
        Err(DriverError::Unsupported)
    }
    fn unlink(&self, _p: &str) -> Result<(), DriverError> {
        Err(DriverError::Unsupported)
    }
    fn read_dir(&self, _p: &str) -> Result<Vec<DirEntry>, DriverError> {
        Err(DriverError::Unsupported)
    }
    fn mkdir(&self, _p: &str) -> Result<(), DriverError> {
        Err(DriverError::Unsupported)
    }
    fn rmdir(&self, _p: &str) -> Result<(), DriverError> {
        Err(DriverError::Unsupported)
    }
    fn rename(&self, _s: &str, _d: &str) -> Result<(), DriverError> {
        Err(DriverError::Unsupported)
    }
    fn statvfs(&self) -> Result<(u64, u64), DriverError> {
        Err(DriverError::Unsupported)
    }
    fn sync(&self) -> Result<(), DriverError> {
        Err(DriverError::Unsupported)
    }
}