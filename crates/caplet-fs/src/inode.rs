//! Inode / file handle representation
//!
//! In CapletRT VFS, an inode is the global identity of a file or
//! directory. File handles are kernel-internal references to
//! open inodes (with a position cursor and access mode).

#![deny(missing_docs)]

use crate::driver::FileMode;

/// Kind of inode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodeKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
    /// Symbolic link (M3+ feature).
    Symlink,
}

/// Inode identifier — unique per mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Inode(pub u32);

/// Open-file handle. Stored in the kernel's per-task handle
/// table.
#[derive(Debug, Clone, Copy)]
pub struct FileHandle {
    /// Inode this handle refers to.
    pub inode: Inode,
    /// Mode the handle was opened with.
    pub mode: FileMode,
    /// Current read/write cursor.
    pub position: u64,
}

impl FileHandle {
    /// Construct a new handle.
    pub fn new(inode: Inode, mode: FileMode) -> Self {
        Self {
            inode,
            mode,
            position: 0,
        }
    }
}