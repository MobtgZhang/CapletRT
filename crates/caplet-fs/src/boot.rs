//! Boot-time mount table bootstrap.
//!
//! A typical CapletRT system mounts a small set of filesystems
//! before the first user task runs. This module provides a
//! single entry point that registers a conventional layout:
//!
//! * `tmp`  — RAMFS, read-write, scratch space
//! * `data` — RAMFS, read-write, persistent scratch (until a
//!   real LittleFS / FAT32 backend replaces the stub)
//! * `log`  — RAMFS, read-only, where the log writer deposits
//!   crash logs
//!
//! Real systems would override these bindings; the helper is a
//! convenience for boards and tests.

use alloc::sync::Arc;

use crate::driver::FsDriver;
use crate::mount::MountTable;
use crate::ramfs::RamFs;

/// Standard mount names. Tests and boards may rely on these.
pub const MOUNT_TMP: &str = "tmp";
/// Persistent data mount.
pub const MOUNT_DATA: &str = "data";
/// Log mount.
pub const MOUNT_LOG: &str = "log";

fn as_dyn_arc(ram: Arc<RamFs>) -> Arc<dyn FsDriver> {
    let r: Arc<dyn FsDriver> = ram;
    r
}

/// Build the standard mount table.
pub fn standard_mount_table() -> (MountTable, Arc<RamFs>, Arc<RamFs>, Arc<RamFs>) {
    let tmp: Arc<RamFs> = RamFs::new();
    let data: Arc<RamFs> = RamFs::new();
    let log: Arc<RamFs> = RamFs::new();

    let mut table = MountTable::new();
    table
        .mount(MOUNT_TMP, as_dyn_arc(tmp.clone()), false)
        .expect("mount tmp");
    table
        .mount(MOUNT_DATA, as_dyn_arc(data.clone()), false)
        .expect("mount data");
    table
        .mount(MOUNT_LOG, as_dyn_arc(log.clone()), true)
        .expect("mount log");
    (table, tmp, data, log)
}

#[allow(dead_code)]
fn _check_compiles(ram: Arc<RamFs>) -> Arc<dyn FsDriver> {
    as_dyn_arc(ram)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::FileMode;
    use crate::path::Path;

    #[test]
    fn boot_mounts_standard_layout() {
        let (table, tmp, _data, _log) = standard_mount_table();
        // tmp is read-write.
        let p = Path::parse("/tmp/hello").unwrap();
        let (mp, sub) = p.split_mount().unwrap();
        assert_eq!(mp, "tmp");
        let h = tmp.open(&sub, FileMode::CreateTruncate).unwrap();
        tmp.write(h, b"hi").unwrap();
        tmp.close(h).unwrap();
        let names: alloc::vec::Vec<&str> = table.mounts().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"tmp"));
        assert!(names.contains(&"data"));
        assert!(names.contains(&"log"));
    }
}