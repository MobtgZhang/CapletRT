//! LittleFS placeholder backend
//!
//! The `littlefs2` crate can be plugged in here. For M2 we expose
//! a stub driver so the mount table & syscall paths are exercised
//! end-to-end without bringing in the real implementation.
//!
//! ## Implementation Plan (M2.x)
//!
//! To implement the real LittleFS driver:
//!
//! 1. Add dependency:
//!    ```toml
//!    # In Cargo.toml of caplet-fs
//!    littlefs2 = "0.4"
//!    ```
//!
//! 2. Implement the storage trait for your flash device:
//!    ```ignore
//!    impl<L> littlefs2::storage::Storage for FlashStorage<L>
//!    where L: FlashInterface
//!    ```
//!
//! 3. Create a wrapper struct:
//!    ```ignore
//!    pub struct LittleFsDriver<S: Storage> {
//!        fs: Mutex<littlefs2::Fs<S>>,
//!    }
//!    ```
//!
//! 4. Implement `FsDriver` trait by delegating to LittleFS methods
//!
//! ## Why LittleFS?
//!
//! LittleFS is designed for embedded flash storage:
//! - Power-failure resilient (copy-on-write)
//! - Built-in wear leveling
//! - Minimal RAM footprint
//! - Ideal for configuration, logs, and small data storage

extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::driver::{DirEntry, DriverError, FileMode, FsDriver, SeekFrom};

/// Stub LittleFS driver. All operations return
/// [`DriverError::Unsupported`].
///
/// This placeholder allows the filesystem abstraction to be exercised
/// without real storage hardware. Production systems should implement
/// the actual LittleFS driver.
pub struct LittleFsPlaceholder;

impl LittleFsPlaceholder {
    /// Wrap in an Arc.
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl FsDriver for LittleFsPlaceholder {
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