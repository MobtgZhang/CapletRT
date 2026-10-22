//! LittleFS v2 backend.
//!
//! The backend stores a real littlefs v2 image in RAM and commits it through
//! the pure-Rust `littlefs2-rust` implementation.  Files are buffered while a
//! handle is open; `flush` and `close` publish the complete file through the
//! normal littlefs metadata/data ordering.  The image can be exported with
//! [`LittleFs::checkpoint_bytes`] and mounted again on the next boot.
//!
//! The implementation deliberately mounts a fresh view for every operation.
//! That keeps the VFS driver `Send + Sync` without exposing the reference-
//! counted internals of the littlefs crate, while still using the real on-disk
//! format for every read, write, allocation and rename.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use caplet_sync::spin::SpinLock;
use littlefs_rust::{Config, FileType, Filesystem, MemoryBlockDevice};

use crate::driver::{DirEntry, DriverError, FileMode, FsDriver, SeekFrom};

/// Geometry used by the in-memory LittleFS volume (256 KiB, 512-byte blocks).
pub const DEFAULT_BLOCK_SIZE: usize = 512;
/// Number of blocks in the default in-memory volume.
pub const DEFAULT_BLOCK_COUNT: usize = 512;
/// Total default volume size in bytes.
pub const DEFAULT_VOLUME_BYTES: usize = DEFAULT_BLOCK_SIZE * DEFAULT_BLOCK_COUNT;

/// A real littlefs v2 driver backed by a serialized image.
pub struct LittleFs {
    state: SpinLock<State>,
}

struct State {
    image: Vec<u8>,
    config: Config,
    handles: BTreeMap<u32, Handle>,
    next_handle: u32,
}

struct Handle {
    path: String,
    data: Vec<u8>,
    pos: usize,
    mode: FileMode,
    dirty: bool,
}

impl LittleFs {
    /// Create and format an empty 256 KiB LittleFS volume.
    pub fn new() -> Arc<Self> {
        Self::new_with_blocks(DEFAULT_BLOCK_COUNT)
    }

    /// Create and format a LittleFS volume with `block_count` 512-byte
    /// blocks.  Embedded targets use this to keep the image (and the
    /// temporary mount buffers) inside a small heap.
    pub fn new_with_blocks(block_count: usize) -> Arc<Self> {
        Arc::new(Self::new_bare_with_blocks(block_count).expect("format in-memory LittleFS volume"))
    }

    /// Create and format a volume, returning formatting errors instead of
    /// panicking.
    pub fn new_bare() -> Result<Self, DriverError> {
        Self::new_bare_with_blocks(DEFAULT_BLOCK_COUNT)
    }

    /// Fallible variant of [`LittleFs::new_with_blocks`].
    pub fn new_bare_with_blocks(block_count: usize) -> Result<Self, DriverError> {
        if block_count < 2 {
            return Err(DriverError::InvalidArgument);
        }
        let cfg = Config {
            block_size: DEFAULT_BLOCK_SIZE,
            block_count,
        };
        let mut device = MemoryBlockDevice::new_erased(cfg).map_err(map_err)?;
        Filesystem::format_device(&mut device).map_err(map_err)?;
        let image = device.as_bytes().to_vec();
        Ok(Self::from_image_unchecked(image, cfg))
    }

    /// Mount an existing littlefs image.
    pub fn from_bytes(bytes: &[u8]) -> Result<Arc<Self>, DriverError> {
        Ok(Arc::new(Self::try_from_bytes(bytes)?))
    }

    /// Mount an existing image and return the driver by value.
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, DriverError> {
        if bytes.is_empty() || bytes.len() % DEFAULT_BLOCK_SIZE != 0 {
            return Err(DriverError::InvalidArgument);
        }
        let config = Config {
            block_size: DEFAULT_BLOCK_SIZE,
            block_count: bytes.len() / DEFAULT_BLOCK_SIZE,
        };
        if config.block_count < 2 {
            return Err(DriverError::InvalidArgument);
        }
        mount_with(config, bytes)?;
        Ok(Self::from_image_unchecked(bytes.to_vec(), config))
    }

    fn from_image_unchecked(image: Vec<u8>, config: Config) -> Self {
        Self {
            state: SpinLock::new(State {
                image,
                config,
                handles: BTreeMap::new(),
                next_handle: 1,
            }),
        }
    }

    /// Export the current image.  Open-but-unflushed handles are intentionally
    /// not included, matching the meaning of a durable checkpoint.
    pub fn image_bytes(&self) -> Vec<u8> {
        self.state.lock().image.clone()
    }

    /// Restore an image, reporting malformed input to the caller.
    pub fn try_restore_from_checkpoint(bytes: &[u8]) -> Result<Self, DriverError> {
        Self::try_from_bytes(bytes)
    }

    fn flush_locked(state: &mut State, id: u32) -> Result<(), DriverError> {
        let (path, data, dirty) = {
            let h = state.handles.get(&id).ok_or(DriverError::InvalidArgument)?;
            (h.path.clone(), h.data.clone(), h.dirty)
        };
        if !dirty {
            return Ok(());
        }

        let mut fs = mount_with(state.config, &state.image)?;
        let exists = fs.as_filesystem().stat(&path).is_ok();
        if exists {
            fs.write_file(&path, &data).map_err(map_err)?;
        } else {
            fs.create_file(&path, &data).map_err(map_err)?;
        }
        fs.sync().map_err(map_err)?;
        let device = fs.into_device();
        state.image.clear();
        state.image.extend_from_slice(device.as_bytes());
        if let Some(h) = state.handles.get_mut(&id) {
            h.dirty = false;
        }
        Ok(())
    }
}

impl Default for LittleFs {
    fn default() -> Self {
        Self::new_bare().expect("format in-memory LittleFS volume")
    }
}

/// Backwards-compatible name used by older board code.
pub type LittleFsDriver = LittleFs;
/// Backwards-compatible alias for the former placeholder.
pub type LittleFsPlaceholder = LittleFs;

impl FsDriver for LittleFs {
    fn open(&self, path: &str, mode: FileMode) -> Result<u32, DriverError> {
        let path = normalize(path)?;
        let mut state = self.state.lock();
        let fs = mount_with(state.config, &state.image)?;
        let existing = match fs.as_filesystem().stat(&path) {
            Ok(e) => Some(e),
            Err(littlefs_rust::Error::NotFound) => None,
            Err(e) => return Err(map_err(e)),
        };
        if let Some(e) = &existing {
            if e.ty == FileType::Dir {
                return Err(DriverError::InvalidArgument);
            }
        }
        if matches!(mode, FileMode::CreateNew) && existing.is_some() {
            return Err(DriverError::AlreadyExists);
        }
        let create = matches!(mode, FileMode::CreateNew | FileMode::CreateTruncate);
        if existing.is_none() && !create {
            return Err(DriverError::NotFound);
        }
        let truncate = matches!(mode, FileMode::CreateTruncate);
        let data = if truncate || existing.is_none() {
            Vec::new()
        } else {
            fs.read_file(&path).map_err(map_err)?
        };
        let pos = if matches!(mode, FileMode::Append) {
            data.len()
        } else {
            0
        };
        let id = state.next_handle;
        state.next_handle = state
            .next_handle
            .checked_add(1)
            .ok_or(DriverError::NoSpace)?;
        state.handles.insert(
            id,
            Handle {
                path,
                data,
                pos,
                mode,
                dirty: truncate || existing.is_none(),
            },
        );
        Ok(id)
    }

    fn close(&self, handle: u32) -> Result<(), DriverError> {
        {
            let mut state = self.state.lock();
            Self::flush_locked(&mut state, handle)?;
            state
                .handles
                .remove(&handle)
                .ok_or(DriverError::InvalidArgument)?;
        }
        Ok(())
    }

    fn read(&self, handle: u32, buf: &mut [u8]) -> Result<usize, DriverError> {
        let state = self.state.lock();
        let h = state
            .handles
            .get(&handle)
            .ok_or(DriverError::InvalidArgument)?;
        if !h.mode.is_read() {
            return Err(DriverError::PermissionDenied);
        }
        if h.pos >= h.data.len() {
            return Ok(0);
        }
        let n = buf.len().min(h.data.len() - h.pos);
        drop(state);
        // Re-acquire because the borrow above cannot outlive the copy below in
        // the no_std borrow checker's view.
        let mut state = self.state.lock();
        let h = state
            .handles
            .get_mut(&handle)
            .ok_or(DriverError::InvalidArgument)?;
        buf[..n].copy_from_slice(&h.data[h.pos..h.pos + n]);
        h.pos += n;
        Ok(n)
    }

    fn write(&self, handle: u32, buf: &[u8]) -> Result<usize, DriverError> {
        let mut state = self.state.lock();
        let h = state
            .handles
            .get_mut(&handle)
            .ok_or(DriverError::InvalidArgument)?;
        if !h.mode.is_write() {
            return Err(DriverError::PermissionDenied);
        }
        if matches!(h.mode, FileMode::Append) {
            h.pos = h.data.len();
        }
        let end = h.pos.checked_add(buf.len()).ok_or(DriverError::NoSpace)?;
        if end > h.data.len() {
            h.data.resize(end, 0);
        }
        h.data[h.pos..end].copy_from_slice(buf);
        h.pos = end;
        h.dirty = true;
        Ok(buf.len())
    }

    fn seek(&self, handle: u32, from: SeekFrom) -> Result<u64, DriverError> {
        let mut state = self.state.lock();
        let h = state
            .handles
            .get_mut(&handle)
            .ok_or(DriverError::InvalidArgument)?;
        let base = match from {
            SeekFrom::Start(v) => i128::from(v),
            SeekFrom::Current(v) => h.pos as i128 + i128::from(v),
            SeekFrom::End(v) => h.data.len() as i128 + i128::from(v),
        };
        if base < 0 || base > u64::MAX as i128 {
            return Err(DriverError::InvalidArgument);
        }
        h.pos = base as usize;
        Ok(base as u64)
    }

    fn flush(&self, handle: u32) -> Result<(), DriverError> {
        let mut state = self.state.lock();
        Self::flush_locked(&mut state, handle)
    }

    fn unlink(&self, path: &str) -> Result<(), DriverError> {
        let path = normalize(path)?;
        let mut state = self.state.lock();
        let mut fs = mount_with(state.config, &state.image)?;
        fs.remove_file(&path).map_err(map_err)?;
        fs.sync().map_err(map_err)?;
        let device = fs.into_device();
        state.image.clear();
        state.image.extend_from_slice(device.as_bytes());
        Ok(())
    }

    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, DriverError> {
        let path = normalize(path)?;
        let state = self.state.lock();
        let fs = mount_with(state.config, &state.image)?;
        let entries = fs.read_dir(&path).map_err(map_err)?;
        Ok(entries
            .into_iter()
            .filter(|e| e.name != "." && e.name != "..")
            .map(|e| DirEntry {
                name: e.name,
                is_dir: e.ty == FileType::Dir,
                size: u64::from(e.size),
            })
            .collect())
    }

    fn mkdir(&self, path: &str) -> Result<(), DriverError> {
        let path = normalize(path)?;
        let mut state = self.state.lock();
        let mut fs = mount_with(state.config, &state.image)?;
        fs.create_dir(&path).map_err(map_err)?;
        fs.sync().map_err(map_err)?;
        let device = fs.into_device();
        state.image.clear();
        state.image.extend_from_slice(device.as_bytes());
        Ok(())
    }

    fn rmdir(&self, path: &str) -> Result<(), DriverError> {
        let path = normalize(path)?;
        let mut state = self.state.lock();
        let mut fs = mount_with(state.config, &state.image)?;
        fs.remove_dir(&path).map_err(map_err)?;
        fs.sync().map_err(map_err)?;
        let device = fs.into_device();
        state.image.clear();
        state.image.extend_from_slice(device.as_bytes());
        Ok(())
    }

    fn rename(&self, src: &str, dst: &str) -> Result<(), DriverError> {
        let src = normalize(src)?;
        let dst = normalize(dst)?;
        let mut state = self.state.lock();
        let mut fs = mount_with(state.config, &state.image)?;
        let is_dir = fs.as_filesystem().stat(&src).map_err(map_err)?.ty == FileType::Dir;
        if is_dir {
            fs.rename_dir(&src, &dst).map_err(map_err)?;
        } else {
            fs.rename_file(&src, &dst).map_err(map_err)?;
        }
        fs.sync().map_err(map_err)?;
        let device = fs.into_device();
        state.image.clear();
        state.image.extend_from_slice(device.as_bytes());
        for h in state.handles.values_mut() {
            if h.path == src {
                h.path = dst.clone();
            } else if h.path.starts_with(&src) && h.path.as_bytes().get(src.len()) == Some(&b'/') {
                h.path = dst.clone() + &h.path[src.len()..];
            }
        }
        Ok(())
    }

    fn statvfs(&self) -> Result<(u64, u64), DriverError> {
        let state = self.state.lock();
        let fs = mount_with(state.config, &state.image)?;
        let used = fs
            .used_blocks()
            .map_err(map_err)?
            .into_iter()
            .filter(|v| *v)
            .count() as u64;
        let block = state.config.block_size as u64;
        let total = state.config.block_count as u64 * block;
        let free = total.saturating_sub(used * block);
        Ok((total, free))
    }

    fn sync(&self) -> Result<(), DriverError> {
        let ids: Vec<u32> = self.state.lock().handles.keys().copied().collect();
        for id in ids {
            self.flush(id)?;
        }
        Ok(())
    }
}

fn mount_with(
    config: Config,
    bytes: &[u8],
) -> Result<littlefs_rust::FilesystemMut<MemoryBlockDevice>, DriverError> {
    let device = MemoryBlockDevice::from_bytes(config, bytes).map_err(map_err)?;
    Filesystem::mount_device_mut(device).map_err(map_err)
}

fn normalize(path: &str) -> Result<String, DriverError> {
    if path.is_empty() {
        return Ok(String::from("/"));
    }
    if !path.starts_with('/') {
        return Err(DriverError::InvalidName);
    }
    let mut out = String::new();
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.len() > 255 {
            return Err(DriverError::InvalidName);
        }
        out.push('/');
        out.push_str(segment);
    }
    if out.is_empty() {
        out.push('/');
    }
    Ok(out)
}

fn map_err(err: littlefs_rust::Error) -> DriverError {
    use littlefs_rust::Error;
    match err {
        Error::NotFound => DriverError::NotFound,
        Error::AlreadyExists => DriverError::AlreadyExists,
        Error::NotDir | Error::IsDir | Error::InvalidPath => DriverError::InvalidArgument,
        Error::NotEmpty => DriverError::InvalidArgument,
        Error::NameTooLong | Error::FileTooLarge => DriverError::InvalidArgument,
        Error::NoSpace => DriverError::NoSpace,
        Error::InvalidConfig | Error::OutOfBounds | Error::Corrupt => DriverError::IoError,
        Error::Utf8 => DriverError::InvalidName,
        Error::BadFileDescriptor => DriverError::InvalidArgument,
        Error::Unsupported => DriverError::Unsupported,
        Error::Io => DriverError::IoError,
    }
}

impl crate::powercut::PowercutAware for LittleFs {
    fn checkpoint_bytes(&self) -> Vec<u8> {
        self.image_bytes()
    }

    fn restore_from_checkpoint(bytes: &[u8]) -> Self {
        Self::try_restore_from_checkpoint(bytes)
            .unwrap_or_else(|_| Self::new_bare().expect("format recovery LittleFS volume"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::powercut::PowercutAware;

    #[test]
    fn format_mount_and_persist() {
        let fs = LittleFs::new();
        let h = fs.open("/sentinel.txt", FileMode::CreateTruncate).unwrap();
        assert_eq!(fs.write(h, b"littlefs-persist").unwrap(), 16);
        fs.flush(h).unwrap();
        fs.close(h).unwrap();

        let bytes = fs.checkpoint_bytes();
        let restored = LittleFs::try_restore_from_checkpoint(&bytes).unwrap();
        let h = restored.open("/sentinel.txt", FileMode::Read).unwrap();
        let mut buf = [0u8; 32];
        let n = restored.read(h, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"littlefs-persist");
        restored.close(h).unwrap();
    }

    #[test]
    fn directories_rename_and_remove() {
        let fs = LittleFs::new();
        fs.mkdir("/logs").unwrap();
        let h = fs.open("/logs/a.txt", FileMode::CreateTruncate).unwrap();
        fs.write(h, b"abc").unwrap();
        fs.close(h).unwrap();
        fs.rename("/logs/a.txt", "/logs/b.txt").unwrap();
        assert!(fs.open("/logs/a.txt", FileMode::Read).is_err());
        let names = fs.read_dir("/logs").unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].name, "b.txt");
        fs.unlink("/logs/b.txt").unwrap();
        fs.rmdir("/logs").unwrap();
    }

    #[test]
    fn append_and_seek() {
        let fs = LittleFs::new();
        let h = fs.open("/a", FileMode::CreateTruncate).unwrap();
        fs.write(h, b"hello").unwrap();
        fs.close(h).unwrap();
        let h = fs.open("/a", FileMode::Append).unwrap();
        fs.write(h, b" world").unwrap();
        fs.close(h).unwrap();
        let h = fs.open("/a", FileMode::Read).unwrap();
        assert_eq!(fs.seek(h, SeekFrom::End(-5)).unwrap(), 6);
        let mut b = [0u8; 5];
        assert_eq!(fs.read(h, &mut b).unwrap(), 5);
        assert_eq!(&b, b"world");
    }

    #[test]
    fn small_geometry_roundtrips_through_checkpoint() {
        let fs = LittleFs::new_with_blocks(128);
        let h = fs.open("/small.txt", FileMode::CreateTruncate).unwrap();
        fs.write(h, b"64k-volume").unwrap();
        fs.close(h).unwrap();
        let bytes = fs.checkpoint_bytes();
        assert_eq!(bytes.len(), 128 * DEFAULT_BLOCK_SIZE);
        let restored = LittleFs::restore_from_checkpoint(&bytes);
        let h = restored.open("/small.txt", FileMode::Read).unwrap();
        let mut b = [0u8; 16];
        let n = restored.read(h, &mut b).unwrap();
        assert_eq!(&b[..n], b"64k-volume");
        restored.close(h).unwrap();
    }
}
