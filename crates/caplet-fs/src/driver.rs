//! Filesystem driver trait.

#![deny(missing_docs)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Open mode for [`FsDriver::open`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMode {
    /// Read-only.
    Read,
    /// Write-only.
    Write,
    /// Read-write.
    ReadWrite,
    /// Append-only (writes always go to EOF).
    Append,
    /// Create-or-truncate.
    CreateTruncate,
    /// Create-or-open; fail if already exists.
    CreateNew,
}

impl FileMode {
    /// `true` if the mode requests write access.
    pub fn is_write(&self) -> bool {
        !matches!(self, FileMode::Read)
    }

    /// `true` if the mode requests read access. Anything
    /// that is not Write-only or Append-only grants read.
    pub fn is_read(&self) -> bool {
        !matches!(self, FileMode::Write | FileMode::Append)
    }
}

/// Driver error variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverError {
    /// Path does not exist.
    NotFound,
    /// Path already exists (when O_EXCL).
    AlreadyExists,
    /// Permission denied.
    PermissionDenied,
    /// Filesystem is read-only.
    ReadOnly,
    /// No space left.
    NoSpace,
    /// Invalid filename (e.g. contains `/` when naming a
    /// sub-entry).
    InvalidName,
    /// I/O error (device gone, CRC mismatch, etc.).
    IoError,
    /// Argument invalid.
    InvalidArgument,
    /// Operation not supported by this driver.
    Unsupported,
}

/// Convenient alias.
pub type FsError = DriverError;

/// Offset semantics for [`FsDriver::seek`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekFrom {
    /// Absolute position from start of file.
    Start(u64),
    /// Relative to current position.
    Current(i64),
    /// Relative to end of file.
    End(i64),
}

/// Single result of [`FsDriver::read_dir`].
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Entry name (single segment, no slashes).
    pub name: String,
    /// `true` for directories.
    pub is_dir: bool,
    /// Size in bytes (for regular files).
    pub size: u64,
}

/// Driver trait. Implementations: ramfs, littlefs, fat32.
pub trait FsDriver: Send + Sync {
    /// Open or create a file at `path` with `mode`.
    fn open(&self, path: &str, mode: FileMode) -> Result<u32, DriverError>;

    /// Close a previously-opened file.
    fn close(&self, handle: u32) -> Result<(), DriverError>;

    /// Read bytes from a file at the current offset.
    fn read(&self, handle: u32, buf: &mut [u8]) -> Result<usize, DriverError>;

    /// Write bytes at the current offset.
    fn write(&self, handle: u32, buf: &[u8]) -> Result<usize, DriverError>;

    /// Seek `handle` according to `from`.
    fn seek(&self, handle: u32, from: SeekFrom) -> Result<u64, DriverError>;

    /// Flush pending writes to storage.
    fn flush(&self, handle: u32) -> Result<(), DriverError>;

    /// Delete the file at `path`.
    fn unlink(&self, path: &str) -> Result<(), DriverError>;

    /// List the contents of `path`.
    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, DriverError>;

    /// Create a directory at `path`.
    fn mkdir(&self, path: &str) -> Result<(), DriverError>;

    /// Remove an empty directory at `path`.
    fn rmdir(&self, path: &str) -> Result<(), DriverError>;

    /// Rename (or move) `src` to `dst`.
    fn rename(&self, src: &str, dst: &str) -> Result<(), DriverError>;

    /// Return total and free bytes on the underlying storage.
    fn statvfs(&self) -> Result<(u64, u64), DriverError>;

    /// Sync all pending writes (analogous to `fsync`).
    fn sync(&self) -> Result<(), DriverError>;
}