//! Virtual filesystem abstraction
//!
//! Mount-points, files, and directories are all represented as
//! **capabilities**: a `Cap<MountPoint>`, `Cap<File>`,
//! `Cap<Directory>`. The VFS layer resolves a path through a
//! `MountTable`, looks up the per-mount driver (LittleFS, FAT32,
//! RAMFS) and dispatches the operation through the driver trait.

#![no_std]
#![deny(missing_docs)]
// MSRV 1.75 predates `is_multiple_of`; the modulo form is intentional.
#![allow(clippy::manual_is_multiple_of)]
// MSRV 1.75 predates `slice::as_chunks`; keep the portable form.
#![allow(clippy::chunks_exact_to_as_chunks)]

extern crate alloc;

pub mod boot;
pub mod driver;
pub mod fat32;
pub mod inode;
pub mod littlefs;
pub mod mount;
pub mod path;
pub mod powercut;
pub mod ramfs;
pub mod syscall;

pub use driver::{DirEntry, DriverError, FileMode, FsDriver, SeekFrom};
pub use inode::{FileHandle, Inode, InodeKind};
pub use mount::{MountId, MountPoint, MountTable, VfsResult};
pub use path::Path;
pub use syscall::{syscall_fs, FsSyscall};

/// Capability error variants returned by VFS operations.
pub use driver::FsError as VfsError;
