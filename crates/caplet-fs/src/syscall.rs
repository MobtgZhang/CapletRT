//! VFS syscalls.

use crate::driver::FileMode;
use crate::path::Path;

/// VFS-scoped syscall numbers. The kernel computes the actual
/// syscall number by adding an offset to these.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsSyscall {
    /// Open or create a file.
    Open = 0,
    /// Close a file.
    Close = 1,
    /// Read from a handle.
    Read = 2,
    /// Write to a handle.
    Write = 3,
    /// Seek within a handle.
    Seek = 4,
    /// Flush a handle.
    Flush = 5,
    /// Delete a file.
    Unlink = 6,
    /// List a directory.
    ReadDir = 7,
    /// Create a directory.
    Mkdir = 8,
    /// Remove a directory.
    Rmdir = 9,
    /// Rename a file.
    Rename = 10,
    /// fsync.
    Sync = 11,
}

impl FsSyscall {
    /// Decode a raw syscall number back into an `FsSyscall`.
    /// Returns `None` if out of range.
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Open,
            1 => Self::Close,
            2 => Self::Read,
            3 => Self::Write,
            4 => Self::Seek,
            5 => Self::Flush,
            6 => Self::Unlink,
            7 => Self::ReadDir,
            8 => Self::Mkdir,
            9 => Self::Rmdir,
            10 => Self::Rename,
            11 => Self::Sync,
            _ => return None,
        })
    }
}

/// Top-level VFS syscall dispatcher. Resolves `path` against the
/// supplied [`MountTable`] and forwards to the underlying driver.
///
/// This is the entry point used by `syscall_dispatch`; it runs
/// with normal kernel privileges (so the caller must already
/// hold the kernel capability).
pub fn syscall_fs(
    table: &crate::mount::MountTable,
    op: FsSyscall,
    path: &str,
    mode: FileMode,
    buf: Option<&mut [u8]>,
    cursor: u64,
) -> Result<u64, crate::mount::MountError> {
    let p = Path::parse(path).map_err(|_| crate::mount::MountError::NoSuchMount)?;
    let (mount, sub) = table.resolve(&p).ok_or(crate::mount::MountError::NoSuchMount)?;
    if mount.read_only && (matches!(op, FsSyscall::Open) || mode.is_write()) {
        return Err(crate::mount::MountError::Driver(
            crate::driver::DriverError::ReadOnly,
        ));
    }
    let driver = mount.driver.as_ref();
    match op {
        FsSyscall::Open => {
            let h = driver.open(sub, mode)?;
            Ok(h as u64)
        }
        FsSyscall::Close => {
            let h = path_to_handle(path)?;
            driver.close(h)?;
            Ok(0)
        }
        FsSyscall::Read => {
            let h = path_to_handle(path)?;
            let buf = buf.unwrap_or(&mut []);
            let n = driver.read(h, buf)?;
            Ok(n as u64)
        }
        FsSyscall::Write => {
            let h = path_to_handle(path)?;
            let bytes = buf.unwrap_or(&mut []);
            let n = driver.write(h, bytes)?;
            Ok(n as u64)
        }
        FsSyscall::Seek => {
            let h = path_to_handle(path)?;
            let pos = driver.seek(h, crate::driver::SeekFrom::Start(cursor))?;
            Ok(pos)
        }
        FsSyscall::Flush => {
            let h = path_to_handle(path)?;
            driver.flush(h)?;
            Ok(0)
        }
        FsSyscall::Unlink => {
            driver.unlink(sub)?;
            Ok(0)
        }
        FsSyscall::ReadDir => {
            let entries = driver.read_dir(sub)?;
            Ok(entries.len() as u64)
        }
        FsSyscall::Mkdir => {
            driver.mkdir(sub)?;
            Ok(0)
        }
        FsSyscall::Rmdir => {
            driver.rmdir(sub)?;
            Ok(0)
        }
        FsSyscall::Rename => {
            // For Rename the caller passes "src\0dst" in the path
            // buffer. Cheap protocol that avoids a syscall-with-
            // two-strings.
            let mut parts = sub.splitn(2, '\0');
            let src = parts.next().unwrap_or("");
            let dst = parts.next().unwrap_or("");
            driver.rename(src, dst)?;
            Ok(0)
        }
        FsSyscall::Sync => {
            driver.sync()?;
            Ok(0)
        }
    }
}

/// Treat the last 8 bytes of the path buffer as a little-endian
/// file handle. Used by the close/read/write/seek/flush paths
/// where the kernel already validated the handle table.
fn path_to_handle(path: &str) -> Result<u32, crate::mount::MountError> {
    let bytes = path.as_bytes();
    if bytes.len() < 8 {
        return Err(crate::mount::MountError::NoSuchMount);
    }
    let n = bytes.len() - 8;
    let mut out = 0u32;
    for i in 0..4 {
        out |= (bytes[n + i] as u32) << (i * 8);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::{DirEntry, DriverError, FsDriver, SeekFrom};
    use crate::mount::MountTable;
    use alloc::sync::Arc;
    use alloc::vec;
    use alloc::vec::Vec;

    /// Trivial in-memory test driver.
    struct StubDriver;
    impl FsDriver for StubDriver {
        fn open(&self, _path: &str, _mode: FileMode) -> Result<u32, DriverError> {
            Ok(7)
        }
        fn close(&self, _h: u32) -> Result<(), DriverError> {
            Ok(())
        }
        fn read(&self, _h: u32, buf: &mut [u8]) -> Result<usize, DriverError> {
            buf[0] = 0xAB;
            Ok(1)
        }
        fn write(&self, _h: u32, buf: &[u8]) -> Result<usize, DriverError> {
            Ok(buf.len())
        }
        fn seek(&self, _h: u32, _from: SeekFrom) -> Result<u64, DriverError> {
            Ok(42)
        }
        fn flush(&self, _h: u32) -> Result<(), DriverError> {
            Ok(())
        }
        fn unlink(&self, _path: &str) -> Result<(), DriverError> {
            Ok(())
        }
        fn read_dir(&self, _path: &str) -> Result<Vec<DirEntry>, DriverError> {
            Ok(Vec::new())
        }
        fn mkdir(&self, _path: &str) -> Result<(), DriverError> {
            Ok(())
        }
        fn rmdir(&self, _path: &str) -> Result<(), DriverError> {
            Ok(())
        }
        fn rename(&self, _s: &str, _d: &str) -> Result<(), DriverError> {
            Ok(())
        }
        fn statvfs(&self) -> Result<(u64, u64), DriverError> {
            Ok((1024, 512))
        }
        fn sync(&self) -> Result<(), DriverError> {
            Ok(())
        }
    }

    #[test]
    fn syscall_open_then_close() {
        let mut t = MountTable::new();
        t.mount("data", Arc::new(StubDriver), false).unwrap();
        let h = syscall_fs(&t, FsSyscall::Open, "/data/foo", FileMode::Read, None, 0).unwrap();
        assert_eq!(h, 7);
    }

    #[test]
    fn rejects_unknown_mount() {
        let t = MountTable::new();
        let res = syscall_fs(&t, FsSyscall::Open, "/missing/x", FileMode::Read, None, 0);
        assert!(matches!(res, Err(crate::mount::MountError::NoSuchMount)));
    }

    #[test]
    fn read_only_mount_rejects_writes() {
        let mut t = MountTable::new();
        t.mount("data", Arc::new(StubDriver), true).unwrap();
        let res = syscall_fs(
            &t,
            FsSyscall::Open,
            "/data/x",
            FileMode::Write,
            None,
            0,
        );
        assert!(res.is_err());
    }
}