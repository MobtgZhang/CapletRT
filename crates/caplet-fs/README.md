# `caplet-fs`

Capability-mediated Virtual Filesystem (VFS) with pluggable
backends.

## Backends

| Backend  | Status     | Description                                                |
|----------|------------|------------------------------------------------------------|
| RAMFS    | Stable     | In-memory; the default for development.                    |
| LittleFS | Implemented | Real littlefs v2 image via `littlefs-rust`; RAM-backed.   |
| FAT32    | Implemented | Real BPB/FAT/directory parser; 8.3 and LFN; RAM-backed.   |

Both persistent backends operate on serialized block images rather than a
direct flash driver. `LittleFs` re-mounts the image for each operation and
buffers an open file until `flush`/`close`; `Fat32` mutates the FAT, directory
entries and cluster chains in place. A board integration supplies the flash
read/write layer by restoring and checkpointing `image_bytes()`.

## Public API

```rust
use caplet_fs::driver::{FsDriver, FileMode};

let fs = RamFs::new();
let h = fs.open("/foo", FileMode::CreateTruncate)?;
fs.write(h, b"hi")?;
fs.close(h)?;
```

## Power-cut resilience

`tests/fixtures/` ships a 256 KiB FAT32 image used by the FAT32 tests.
`powercut.rs` defines the checkpoint/reopen harness used to verify that a
backend can be reconstructed from its durable image after a power cut.

## Status

Implemented for host and RAM-backed board images. Direct on-chip flash
 wear-levelling, DMA flash access, and multi-mount concurrency are board-level
integration work and are not provided by these two backends.
