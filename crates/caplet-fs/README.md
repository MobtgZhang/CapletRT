# `caplet-fs`

Capability-mediated Virtual Filesystem (VFS) with pluggable
backends.

## Backends

| Backend  | Status     | Description                              |
|----------|------------|------------------------------------------|
| RAMFS    | Stable     | In-memory; the default for development.  |
| LittleFS | Stub       | Placeholder; real driver in Phase 6.2.   |
| FAT32    | Stub       | Placeholder; real driver in Phase 6.3.   |

## Public API

```rust
use caplet_fs::driver::{FsDriver, FileMode};

let fs = RamFs::new();
let h = fs.open("/foo", FileMode::CreateTruncate)?;
fs.write(h, b"hi")?;
fs.close(h)?;
```

## Power-cut resilience

`tests/fixtures/` ships an empty 256 KiB FAT32 image used
by the upcoming `littlefs` driver. `powercut.rs` defines
the harness future LittleFS implementations must satisfy.

## Status

In progress. Placeholders + the VFS abstraction are in place;
concrete power-loss-aware drivers arrive in Phase 6.2/6.3.