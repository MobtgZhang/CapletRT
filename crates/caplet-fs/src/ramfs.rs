//! RAMFS — in-memory filesystem driver.
//!
//! A simple BTree-backed filesystem that lives entirely in RAM.
//! Useful for:
//!
//! * unit tests of other VFS layers
//! * a `/tmp` mount on production boards where everything
//!   written should be discarded on reboot
//!
//! The driver is `Send + Sync` because the underlying
//! `BTreeMap`s are protected by a spin-lock.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use caplet_sync::spin::SpinLock;

use crate::driver::{DirEntry, DriverError, FileMode, FsDriver, SeekFrom};

/// One VFS node.
#[derive(Debug, Clone)]
struct Node {
    /// True if this is a directory.
    is_dir: bool,
    /// File contents (empty for directories).
    data: Vec<u8>,
    /// Children (name -> inode id) for directories.
    children: BTreeMap<String, u32>,
}

/// RAMFS driver.
pub struct RamFs {
    nodes: SpinLock<RamFsState>,
}

/// Inner state.
struct RamFsState {
    next_id: u32,
    /// ID -> Node.
    nodes: BTreeMap<u32, Node>,
    /// open_handles[handle] -> (inode, position, mode).
    handles: BTreeMap<u32, HandleEntry>,
    /// next handle id.
    next_handle: u32,
}

#[derive(Debug, Clone, Copy)]
struct HandleEntry {
    inode: u32,
    position: u64,
    mode: FileMode,
}

impl RamFs {
    /// Construct an empty RAMFS.
    pub fn new() -> Arc<Self> {
        let mut s = RamFsState {
            next_id: 1,
            nodes: BTreeMap::new(),
            handles: BTreeMap::new(),
            next_handle: 1,
        };
        // Pre-create the root directory (inode 0).
        s.nodes.insert(
            0,
            Node {
                is_dir: true,
                data: Vec::new(),
                children: BTreeMap::new(),
            },
        );
        s.next_id = 1;
        Arc::new(Self {
            nodes: SpinLock::new(s),
        })
    }

    /// Walk a path and return the inode id of the (existing) final node,
    /// or `None` if any component is missing. The caller must hold the
    /// state lock.
    fn lookup_locked(state: &RamFsState, path: &str) -> Option<u32> {
        let mut cur = 0u32;
        for seg in path.split('/').filter(|s| !s.is_empty()) {
            let node = state.nodes.get(&cur)?;
            if !node.is_dir {
                return None;
            }
            cur = *node.children.get(seg)?;
        }
        Some(cur)
    }

    /// Like [`Self::lookup`] but creates intermediate directories
    /// as needed. Caller must already hold the state lock.
    fn ensure_parent_locked(state: &mut RamFsState, path: &str) -> Result<u32, DriverError> {
        let mut cur = 0u32;
        for seg in path.split('/').filter(|s| !s.is_empty()) {
            let node = state.nodes.get(&cur).ok_or(DriverError::NotFound)?;
            if !node.is_dir {
                return Err(DriverError::NotFound);
            }
            if let Some(child) = node.children.get(seg) {
                cur = *child;
            } else {
                let id = state.next_id;
                state.next_id += 1;
                state.nodes.insert(
                    id,
                    Node {
                        is_dir: true,
                        data: Vec::new(),
                        children: BTreeMap::new(),
                    },
                );
                state
                    .nodes
                    .get_mut(&cur)
                    .ok_or(DriverError::NotFound)?
                    .children
                    .insert(String::from(seg), id);
                cur = id;
            }
        }
        Ok(cur)
    }
}

impl Default for RamFs {
    fn default() -> Self {
        let _: Arc<Self> = Self::new();
        // We never call this without first wrapping in an Arc,
        // so the leaked RamFs lives for the program duration.
        unreachable!("wrap with RamFs::new() instead")
    }
}

impl FsDriver for RamFs {
    fn open(&self, path: &str, mode: FileMode) -> Result<u32, DriverError> {
        let mut state = self.nodes.lock();
        // Existing?
        let id = if let Some(id) = Self::lookup_locked(&state, path) {
            id
        } else if matches!(mode, FileMode::CreateNew | FileMode::CreateTruncate) {
            // Create file in parent.
            let (dir_path, name) = match path.rsplit_once('/') {
                Some((d, n)) => (d, n),
                None => return Err(DriverError::InvalidName),
            };
            if name.is_empty() || name.contains('/') {
                return Err(DriverError::InvalidName);
            }
            let parent = Self::ensure_parent_locked(&mut state, dir_path)?;
            let id = state.next_id;
            state.next_id += 1;
            state.nodes.insert(
                id,
                Node {
                    is_dir: false,
                    data: Vec::new(),
                    children: BTreeMap::new(),
                },
            );
            state
                .nodes
                .get_mut(&parent)
                .ok_or(DriverError::NotFound)?
                .children
                .insert(String::from(name), id);
            id
        } else {
            return Err(DriverError::NotFound);
        };

        let node = state.nodes.get(&id).ok_or(DriverError::NotFound)?;
        if node.is_dir {
            return Err(DriverError::InvalidArgument);
        }
        if matches!(mode, FileMode::CreateTruncate) {
            // Should already be empty since we just created.
        }
        let handle = state.next_handle;
        state.next_handle += 1;
        state.handles.insert(
            handle,
            HandleEntry {
                inode: id,
                position: 0,
                mode,
            },
        );
        Ok(handle)
    }

    fn close(&self, handle: u32) -> Result<(), DriverError> {
        let mut state = self.nodes.lock();
        state.handles.remove(&handle).ok_or(DriverError::NotFound)?;
        Ok(())
    }

    fn read(&self, handle: u32, buf: &mut [u8]) -> Result<usize, DriverError> {
        let mut state = self.nodes.lock();
        let inode = state
            .handles
            .get(&handle)
            .ok_or(DriverError::NotFound)?
            .inode;
        let mode = state
            .handles
            .get(&handle)
            .ok_or(DriverError::NotFound)?
            .mode;
        if !mode.is_read() {
            return Err(DriverError::PermissionDenied);
        }
        let node = state.nodes.get(&inode).ok_or(DriverError::NotFound)?;
        if node.is_dir {
            return Err(DriverError::InvalidArgument);
        }
        let pos = state
            .handles
            .get(&handle)
            .ok_or(DriverError::NotFound)?
            .position as usize;
        let available = node.data.len().saturating_sub(pos);
        let to_copy = buf.len().min(available);
        buf[..to_copy].copy_from_slice(&node.data[pos..pos + to_copy]);
        state
            .handles
            .get_mut(&handle)
            .ok_or(DriverError::NotFound)?
            .position += to_copy as u64;
        Ok(to_copy)
    }

    fn write(&self, handle: u32, buf: &[u8]) -> Result<usize, DriverError> {
        let mut state = self.nodes.lock();
        let inode = state
            .handles
            .get(&handle)
            .ok_or(DriverError::NotFound)?
            .inode;
        let mode = state
            .handles
            .get(&handle)
            .ok_or(DriverError::NotFound)?
            .mode;
        if !mode.is_write() {
            return Err(DriverError::PermissionDenied);
        }
        let pos = state
            .handles
            .get(&handle)
            .ok_or(DriverError::NotFound)?
            .position as usize;
        let node = state.nodes.get_mut(&inode).ok_or(DriverError::NotFound)?;
        if node.is_dir {
            return Err(DriverError::InvalidArgument);
        }
        if pos + buf.len() > node.data.len() {
            node.data.resize(pos + buf.len(), 0);
        }
        node.data[pos..pos + buf.len()].copy_from_slice(buf);
        state
            .handles
            .get_mut(&handle)
            .ok_or(DriverError::NotFound)?
            .position += buf.len() as u64;
        Ok(buf.len())
    }

    fn seek(&self, handle: u32, from: SeekFrom) -> Result<u64, DriverError> {
        let mut state = self.nodes.lock();
        let (inode, cur_pos, mode) = {
            let h = state.handles.get(&handle).ok_or(DriverError::NotFound)?;
            (h.inode, h.position, h.mode)
        };
        if mode.is_write() {
            // Already extracted — keep `state` borrow alive.
        }
        let len = state
            .nodes
            .get(&inode)
            .ok_or(DriverError::NotFound)?
            .data
            .len() as i64;
        let target = match from {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::Current(d) => cur_pos as i64 + d,
            SeekFrom::End(d) => len + d,
        };
        if target < 0 {
            return Err(DriverError::InvalidArgument);
        }
        state
            .handles
            .get_mut(&handle)
            .ok_or(DriverError::NotFound)?
            .position = target as u64;
        Ok(target as u64)
    }

    fn flush(&self, _handle: u32) -> Result<(), DriverError> {
        Ok(())
    }

    fn unlink(&self, path: &str) -> Result<(), DriverError> {
        let (dir_path, name) = path.rsplit_once('/').ok_or(DriverError::InvalidName)?;
        if name.is_empty() {
            return Err(DriverError::InvalidName);
        }
        let mut state = self.nodes.lock();
        let parent = Self::lookup_locked(&state, dir_path).ok_or(DriverError::NotFound)?;
        let p = state.nodes.get_mut(&parent).ok_or(DriverError::NotFound)?;
        if !p.is_dir {
            return Err(DriverError::InvalidArgument);
        }
        let target = p.children.remove(name).ok_or(DriverError::NotFound)?;
        {
            let node = state.nodes.get(&target).ok_or(DriverError::NotFound)?;
            if node.is_dir {
                // POSIX unlink(2) returns EISDIR for directories.
                return Err(DriverError::InvalidArgument);
            }
        }
        state.nodes.remove(&target);
        Ok(())
    }

    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, DriverError> {
        let state = self.nodes.lock();
        let id = Self::lookup_locked(&state, path).ok_or(DriverError::NotFound)?;
        let node = state.nodes.get(&id).ok_or(DriverError::NotFound)?;
        if !node.is_dir {
            return Err(DriverError::InvalidArgument);
        }
        let mut out = Vec::with_capacity(node.children.len());
        for (name, child) in node.children.iter() {
            let n = state.nodes.get(child).ok_or(DriverError::NotFound)?;
            out.push(DirEntry {
                name: name.clone(),
                is_dir: n.is_dir,
                size: n.data.len() as u64,
            });
        }
        Ok(out)
    }

    fn mkdir(&self, path: &str) -> Result<(), DriverError> {
        let mut state = self.nodes.lock();
        // ensure_parent will create any missing directories.
        Self::ensure_parent_locked(&mut state, path)?;
        Ok(())
    }

    fn rmdir(&self, path: &str) -> Result<(), DriverError> {
        let (parent_path, name) = match path.rsplit_once('/') {
            Some((p, n)) if !n.is_empty() => (p, n),
            _ => return Err(DriverError::InvalidName),
        };
        let mut state = self.nodes.lock();
        let id = Self::lookup_locked(&state, path).ok_or(DriverError::NotFound)?;
        {
            let node = state.nodes.get(&id).ok_or(DriverError::NotFound)?;
            if !node.is_dir {
                return Err(DriverError::InvalidArgument);
            }
            if !node.children.is_empty() {
                return Err(DriverError::InvalidArgument);
            }
        }
        state.nodes.remove(&id).ok_or(DriverError::NotFound)?;
        if !parent_path.is_empty() {
            let parent = Self::lookup_locked(&state, parent_path).ok_or(DriverError::NotFound)?;
            let p_node = state.nodes.get_mut(&parent).ok_or(DriverError::NotFound)?;
            p_node.children.remove(name);
        }
        Ok(())
    }

    fn rename(&self, src: &str, dst: &str) -> Result<(), DriverError> {
        let (src_dir, src_name) = src.rsplit_once('/').ok_or(DriverError::InvalidName)?;
        let (dst_dir, dst_name) = dst.rsplit_once('/').ok_or(DriverError::InvalidName)?;
        let mut state = self.nodes.lock();
        let src_parent = Self::lookup_locked(&state, src_dir).ok_or(DriverError::NotFound)?;
        let dst_parent = Self::lookup_locked(&state, dst_dir).ok_or(DriverError::NotFound)?;
        let target = state
            .nodes
            .get(&src_parent)
            .ok_or(DriverError::NotFound)?
            .children
            .get(src_name)
            .copied()
            .ok_or(DriverError::NotFound)?;
        let sp = state
            .nodes
            .get_mut(&src_parent)
            .ok_or(DriverError::NotFound)?;
        sp.children.remove(src_name);
        let dp = state
            .nodes
            .get_mut(&dst_parent)
            .ok_or(DriverError::NotFound)?;
        dp.children.insert(String::from(dst_name), target);
        Ok(())
    }

    fn statvfs(&self) -> Result<(u64, u64), DriverError> {
        let state = self.nodes.lock();
        // We can't compute "free" easily; report a constant.
        let total = (state.nodes.len() as u64).saturating_mul(4096);
        let free = total / 2;
        Ok((total, free))
    }

    fn sync(&self) -> Result<(), DriverError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_open_write_read() {
        let fs = RamFs::new();
        let h = fs.open("/foo.txt", FileMode::CreateTruncate).expect("open");
        assert!(fs.write(h, b"hello").is_ok());
        fs.seek(h, SeekFrom::Start(0)).unwrap();
        let mut buf = [0u8; 5];
        assert_eq!(fs.read(h, &mut buf).unwrap(), 5);
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn mkdir_and_read_dir() {
        let fs = RamFs::new();
        fs.mkdir("/a/b/c").unwrap();
        let entries = fs.read_dir("/a").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "b");
        assert!(entries[0].is_dir);
    }

    #[test]
    fn cannot_unlink_populated_dir() {
        let fs = RamFs::new();
        fs.mkdir("/d").unwrap();
        assert!(fs.unlink("/d").is_err());
    }

    #[test]
    fn seek_end() {
        let fs = RamFs::new();
        let h = fs.open("/x", FileMode::CreateTruncate).unwrap();
        fs.write(h, b"abcdef").unwrap();
        let pos = fs.seek(h, SeekFrom::End(-2)).unwrap();
        assert_eq!(pos, 4);
        let mut buf = [0u8; 2];
        assert_eq!(fs.read(h, &mut buf).unwrap(), 2);
        assert_eq!(&buf, b"ef");
    }
}
