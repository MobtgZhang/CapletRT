//! FAT32 interoperability tests against images produced by the host
//! `mkfs.fat` utility.
//!
//! These tests exercise the driver against a filesystem created by an
//! independent implementation rather than by `Fat32::format`.  When the
//! host tooling is unavailable the tests skip instead of failing, so the
//! suite still runs on minimal CI images.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use caplet_fs::driver::{FileMode, FsDriver};
use caplet_fs::fat32::Fat32;
use caplet_fs::powercut::PowercutAware;

const IMAGE_BYTES: u64 = 64 * 1024 * 1024;
const PAYLOAD: &[u8] = b"caplet-fat32-interop-payload";

fn temp_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "caplet-fs-{}-{}-{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p
}

fn tool(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from("/usr/sbin").join(name);
    if path.exists() {
        return Some(path);
    }
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", name))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

fn make_image(path: &Path) -> bool {
    let Some(mkfs) = tool("mkfs.fat") else {
        return false;
    };
    let file = match fs::File::create(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    if file.set_len(IMAGE_BYTES).is_err() {
        return false;
    }
    drop(file);
    Command::new(mkfs)
        .args(["-F", "32", "-s", "1", "-S", "512", "-n", "CAPLET"])
        .arg(path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn read_handle(fs: &impl FsDriver, path: &str) -> Vec<u8> {
    let h = fs.open(path, FileMode::Read).expect("open for read");
    let mut out = Vec::new();
    let mut buf = [0u8; 128];
    loop {
        let n = fs.read(h, &mut buf).expect("read");
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }
    fs.close(h).expect("close");
    out
}

#[test]
fn mounts_host_formatted_fat32_and_roundtrips() {
    let path = temp_path("mkfs");
    if !make_image(&path) {
        eprintln!("skipping FAT32 interop test: mkfs.fat unavailable");
        let _ = fs::remove_file(&path);
        return;
    }

    let bytes = fs::read(&path).expect("read image");
    let volume = Fat32::try_from_bytes(&bytes).expect("mount mkfs.fat image");

    let h = volume
        .open("/sentinel.txt", FileMode::CreateTruncate)
        .expect("create sentinel");
    assert_eq!(volume.write(h, PAYLOAD).expect("write"), PAYLOAD.len());
    volume.flush(h).expect("flush");
    volume.close(h).expect("close");

    assert_eq!(read_handle(&volume, "/sentinel.txt"), PAYLOAD);

    volume.mkdir("/logs").expect("mkdir");
    let h = volume
        .open("/logs/current.log", FileMode::CreateTruncate)
        .expect("create log");
    volume.write(h, b"line-1\n").expect("write log");
    volume.close(h).expect("close log");
    volume
        .rename("/logs/current.log", "/logs/archive.log")
        .expect("rename");
    assert_eq!(read_handle(&volume, "/logs/archive.log"), b"line-1\n");

    let names = volume.read_dir("/").expect("read root");
    assert!(names.iter().any(|e| e.name == "sentinel.txt"));
    assert!(names.iter().any(|e| e.name == "logs" && e.is_dir));

    let (total, free) = volume.statvfs().expect("statvfs");
    assert!(total > 32 * 1024 * 1024);
    assert!(free <= total);

    // Persisted bytes must remain readable by the host filesystem checker.
    let checkpoint = volume.checkpoint_bytes();
    let out_path = temp_path("checkpoint");
    fs::write(&out_path, &checkpoint).expect("write checkpoint");
    if let Some(fsck) = tool("fsck.fat") {
        let out = Command::new(fsck)
            .arg("-n")
            .arg(&out_path)
            .output()
            .expect("run fsck.fat");
        assert!(
            out.status.success(),
            "fsck.fat rejected image:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    if let Some(mdir) = tool("mdir") {
        let out = Command::new(mdir)
            .args(["-i", out_path.to_str().unwrap(), "::/"])
            .output()
            .expect("run mdir");
        assert!(out.status.success(), "mdir rejected image");
        let listing = String::from_utf8_lossy(&out.stdout);
        assert!(listing.contains("SENTINEL"), "listing: {listing}");
    }

    // A checkpoint restored byte-for-byte must expose the same tree.
    let restored = Fat32::try_restore_from_checkpoint(&checkpoint).expect("restore");
    assert_eq!(read_handle(&restored, "/sentinel.txt"), PAYLOAD);
    assert_eq!(read_handle(&restored, "/logs/archive.log"), b"line-1\n");

    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&out_path);
}

#[test]
fn littlefs_checkpoint_roundtrips_and_rejects_corruption() {
    use caplet_fs::littlefs::LittleFs;

    let volume = LittleFs::new();
    let h = volume
        .open("/boot.cfg", FileMode::CreateTruncate)
        .expect("create");
    volume.write(h, b"board=caplet\n").expect("write");
    volume.close(h).expect("close");

    let bytes = volume.checkpoint_bytes();
    let restored = LittleFs::try_restore_from_checkpoint(&bytes).expect("restore");
    assert_eq!(read_handle(&restored, "/boot.cfg"), b"board=caplet\n");

    let mut truncated = bytes.clone();
    truncated.truncate(bytes.len() / 2);
    assert!(LittleFs::try_restore_from_checkpoint(&truncated).is_err());

    let mut corrupt = bytes;
    let mid = corrupt.len() / 2;
    if let Some(byte) = corrupt.get_mut(mid) {
        *byte ^= 0xff;
    }
    // Corruption is detected either at mount time, or the volume falls back
    // to an empty-but-valid filesystem; it must never panic.
    if let Ok(recovered) = LittleFs::try_restore_from_checkpoint(&corrupt) {
        let _ = recovered.read_dir("/");
    }
}

#[test]
fn committed_fixture_mounts_as_fat32() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.fat32");
    let bytes = fs::read(&path).expect("read committed FAT32 fixture");
    let volume = Fat32::try_from_bytes(&bytes).expect("mount committed fixture");

    let entries = volume.read_dir("/").expect("read fixture root");
    assert!(
        entries.is_empty(),
        "fixture should start empty: {entries:?}"
    );

    let h = volume
        .open("/fixture-check.txt", FileMode::CreateTruncate)
        .expect("create in fixture");
    volume.write(h, b"fixture-ok").expect("write");
    volume.close(h).expect("close");
    assert_eq!(read_handle(&volume, "/fixture-check.txt"), b"fixture-ok");

    if let Some(fsck) = tool("fsck.fat") {
        let out_path = temp_path("fixture-check");
        fs::write(&out_path, volume.checkpoint_bytes()).expect("write image");
        let out = Command::new(fsck)
            .arg("-n")
            .arg(&out_path)
            .output()
            .expect("run fsck.fat");
        assert!(
            out.status.success(),
            "fsck.fat rejected fixture after write:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let _ = fs::remove_file(&out_path);
    }
}
