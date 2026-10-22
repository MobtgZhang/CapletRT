//! Power-cut resilience tests for the FS layer.
//!
//! The simulator applies a sequence of acknowledged writes, snapshots the
//! durable backing bytes, drops the driver and mounts a fresh instance from
//! those bytes.  This models a sudden power loss where the only state that
//! survives is what reached the block device before the cut.
//!
//! Both real backends participate: [`LittleFs`](crate::littlefs::LittleFs)
//! exports its littlefs v2 image and [`Fat32`](crate::fat32::Fat32) exports
//! the block image with FAT/FSInfo metadata already synchronised.
//! Unflushed handles intentionally do *not* appear in a checkpoint, which is
//! the same guarantee a real block device offers.

extern crate alloc;

use alloc::vec::Vec;

use crate::driver::{FileMode, FsDriver};

/// Outcome of one powercut simulation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowercutOutcome {
    /// All written bytes are intact after remount.
    Intact,
    /// Bytes truncated to the last checkpoint (expected with
    /// some journaling modes).
    Truncated {
        /// Bytes preserved.
        kept: usize,
        /// Bytes lost.
        lost: usize,
    },
    /// All post-checkpoint bytes lost.
    Lost,
}

/// Driver-extension hook used by the simulator.
pub trait PowercutAware {
    /// Return the bytes that would survive a power cut right
    /// now. Implementations of the real backends map this to a
    /// checkpoint or journal flush.
    fn checkpoint_bytes(&self) -> Vec<u8>;

    /// Reconstruct a driver instance from previously snapshotted
    /// bytes.  Implementations must return a usable (possibly empty)
    /// volume rather than panicking when the snapshot is damaged.
    fn restore_from_checkpoint(bytes: &[u8]) -> Self;
}

/// Run a powercut simulation against an in-memory driver.
///
/// * `fs` — the FS driver under test
/// * `writes` — sequence of `(path, mode, payload)` to apply
///   before the cut
/// * `read_after_path` — path to read after remount to compare
/// * `expected_after` — expected content after the cut
pub fn simulate_powercut<D>(
    fs: &D,
    writes: &[(&str, FileMode, &[u8])],
    read_after_path: &str,
    expected_after: &[u8],
) -> PowercutOutcome
where
    D: PowercutAware + FsDriver,
{
    // Apply and acknowledge every write (flush + close) before the cut.
    let mut handles: Vec<u32> = Vec::new();
    for (path, mode, payload) in writes {
        let h = match fs.open(path, *mode) {
            Ok(h) => h,
            Err(_) => continue,
        };
        let _ = fs.write(h, payload);
        let _ = fs.flush(h);
        handles.push(h);
    }
    for h in handles.iter() {
        let _ = fs.close(*h);
    }
    // Snapshot the durable bytes, then remount from scratch.
    let snap = fs.checkpoint_bytes();
    let restored = D::restore_from_checkpoint(&snap);
    let h = match restored.open(read_after_path, FileMode::Read) {
        Ok(h) => h,
        Err(_) => return PowercutOutcome::Lost,
    };
    let mut buf = [0u8; 256];
    let n = match restored.read(h, &mut buf) {
        Ok(n) => n,
        Err(_) => return PowercutOutcome::Lost,
    };
    let _ = restored.close(h);
    if &buf[..n] == expected_after {
        PowercutOutcome::Intact
    } else {
        PowercutOutcome::Truncated {
            kept: n,
            lost: expected_after.len().saturating_sub(n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::DriverError;
    use crate::fat32::Fat32;
    use crate::littlefs::LittleFs;

    fn read_all(fs: &impl FsDriver, path: &str) -> Option<Vec<u8>> {
        let h = fs.open(path, FileMode::Read).ok()?;
        let mut out = Vec::new();
        let mut buf = [0u8; 128];
        loop {
            match fs.read(h, &mut buf) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(_) => return None,
            }
        }
        let _ = fs.close(h);
        Some(out)
    }

    #[test]
    fn littlefs_acknowledged_write_survives_powercut() {
        let fs = LittleFs::new();
        let outcome = simulate_powercut(
            &*fs,
            &[
                ("/boot.cfg", FileMode::CreateTruncate, b"board=caplet\n"),
                ("/logs/boot.log", FileMode::CreateTruncate, b"started\n"),
            ],
            "/boot.cfg",
            b"board=caplet\n",
        );
        assert_eq!(outcome, PowercutOutcome::Intact);
    }

    #[test]
    fn unflushed_littlefs_write_is_not_durable() {
        let fs = LittleFs::new();
        let h = fs.open("/pending.txt", FileMode::CreateTruncate).unwrap();
        fs.write(h, b"not-acknowledged").unwrap();
        // No flush/close: the handle state has not reached the image.
        let snap = fs.checkpoint_bytes();
        let restored = LittleFs::restore_from_checkpoint(&snap);
        assert_eq!(
            restored.open("/pending.txt", FileMode::Read).err(),
            Some(DriverError::NotFound)
        );
    }

    #[test]
    fn truncated_littlefs_checkpoint_is_rejected() {
        let fs = LittleFs::new();
        let h = fs.open("/a", FileMode::CreateTruncate).unwrap();
        fs.write(h, b"abc").unwrap();
        fs.close(h).unwrap();
        let mut bytes = fs.checkpoint_bytes();
        bytes.truncate(bytes.len() / 2);
        assert!(LittleFs::try_restore_from_checkpoint(&bytes).is_err());
    }

    #[test]
    fn corrupt_littlefs_snapshot_restores_to_usable_volume() {
        let fs = LittleFs::new();
        let h = fs.open("/a", FileMode::CreateTruncate).unwrap();
        fs.write(h, b"abc").unwrap();
        fs.close(h).unwrap();
        let mut bytes = fs.checkpoint_bytes();
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xff;
        // The trait contract is "never panic": either the image mounts or the
        // implementation falls back to a freshly formatted volume.
        let restored = LittleFs::restore_from_checkpoint(&bytes);
        let h = restored.open("/fresh", FileMode::CreateTruncate).unwrap();
        assert_eq!(restored.write(h, b"ok").unwrap(), 2);
        restored.close(h).unwrap();
        assert_eq!(read_all(&restored, "/fresh").as_deref(), Some(&b"ok"[..]));
    }

    #[test]
    fn fat32_acknowledged_write_survives_powercut() {
        let fs = Fat32::format(64 * 1024 * 1024, 1).unwrap();
        let outcome = simulate_powercut(
            &fs,
            &[("/sentinel.bin", FileMode::CreateTruncate, b"fat32-durable")],
            "/sentinel.bin",
            b"fat32-durable",
        );
        assert_eq!(outcome, PowercutOutcome::Intact);
    }

    #[test]
    fn truncated_fat32_checkpoint_is_rejected() {
        let fs = Fat32::format(64 * 1024 * 1024, 1).unwrap();
        let mut bytes = fs.checkpoint_bytes();
        bytes.truncate(bytes.len() / 2);
        assert!(Fat32::try_restore_from_checkpoint(&bytes).is_err());
    }
}
