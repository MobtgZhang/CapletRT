//! Boot-time mount table bootstrap.
//!
//! A typical CapletRT system mounts a small set of filesystems
//! before the first user task runs. This module provides a
//! single entry point that registers a conventional layout:
//!
//! * `tmp`  — RAMFS, read-write scratch space
//! * `data` — real LittleFS v2 volume (64 KiB), read-write and
//!   persistent for the lifetime of the driver image.  The small
//!   geometry keeps copies of the image inside a Cortex-M heap.
//! * `log`  — RAMFS, read-only, where the log writer deposits
//!   crash logs
//!
//! Boards that need a second persistent volume can mount another
//! [`LittleFs`] instance directly; this helper keeps the common
//! layout in one place.

use alloc::sync::Arc;

use crate::driver::FsDriver;
use crate::littlefs::LittleFs;
use crate::mount::MountTable;
use crate::ramfs::RamFs;

/// Standard mount names. Tests and boards may rely on these.
pub const MOUNT_TMP: &str = "tmp";
/// Persistent data mount.
pub const MOUNT_DATA: &str = "data";
/// Log mount.
pub const MOUNT_LOG: &str = "log";

/// Standard mount table together with the three driver handles that keep
/// the mounted backends alive.
pub type StandardMountTable = (
    MountTable,
    Arc<dyn FsDriver>,
    Arc<dyn FsDriver>,
    Arc<dyn FsDriver>,
);

/// Build the standard mount table.
///
/// The returned driver handles keep the backends alive; dropping one before
/// mounting is complete removes that mount from the table.
pub fn standard_mount_table() -> StandardMountTable {
    let tmp: Arc<dyn FsDriver> = RamFs::new();
    let data: Arc<dyn FsDriver> = LittleFs::new_with_blocks(128);
    let log: Arc<dyn FsDriver> = RamFs::new();

    let mut table = MountTable::new();
    table
        .mount(MOUNT_TMP, tmp.clone(), false)
        .expect("mount tmp");
    table
        .mount(MOUNT_DATA, data.clone(), false)
        .expect("mount data");
    table
        .mount(MOUNT_LOG, log.clone(), true)
        .expect("mount log");
    (table, tmp, data, log)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::FileMode;

    #[test]
    fn boot_mounts_standard_layout() {
        let (table, tmp, data, _log) = standard_mount_table();
        let names: alloc::vec::Vec<&str> = table.mounts().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"tmp"));
        assert!(names.contains(&"data"));
        assert!(names.contains(&"log"));

        // tmp is RAMFS scratch space.
        let h = tmp.open("/hello", FileMode::CreateTruncate).unwrap();
        tmp.write(h, b"hi").unwrap();
        tmp.close(h).unwrap();

        // data is a real LittleFS volume that round-trips through reopen.
        let h = data.open("/persist.cfg", FileMode::CreateTruncate).unwrap();
        data.write(h, b"durable").unwrap();
        data.flush(h).unwrap();
        data.close(h).unwrap();
        let h = data.open("/persist.cfg", FileMode::Read).unwrap();
        let mut buf = [0u8; 16];
        let n = data.read(h, &mut buf).unwrap();
        data.close(h).unwrap();
        assert_eq!(&buf[..n], b"durable");
        let entries = data.read_dir("/").unwrap();
        assert!(entries.iter().any(|e| e.name == "persist.cfg"));
    }
}
