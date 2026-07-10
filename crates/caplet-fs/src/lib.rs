//! Virtual filesystem abstraction
//!
//! Mount-points, files, and directories are all represented as
//! **capabilities**: a `Cap<MountPoint>`, `Cap<File>`,
//! `Cap<Directory>`. The VFS layer resolves a path through a
//! `MountTable`, looks up the per-mount driver (LittleFS, FAT32,
//! RAMFS) and dispatches the operation through the driver trait.

#![no_std]
#![deny(missing_docs)]

extern crate alloc;

pub mod mount;
pub mod syscall;
pub mod driver;
pub mod path;
pub mod inode;
pub mod ramfs;
pub mod littlefs;
pub mod fat32;
pub mod boot;

pub use driver::{DirEntry, DriverError, FileMode, FsDriver, SeekFrom};
pub use inode::{FileHandle, Inode, InodeKind};
pub use mount::{MountId, MountPoint, MountTable, VfsResult};
pub use path::Path;
pub use syscall::{syscall_fs, FsSyscall};

/// Capability error variants returned by VFS operations.
pub use driver::FsError as VfsError;