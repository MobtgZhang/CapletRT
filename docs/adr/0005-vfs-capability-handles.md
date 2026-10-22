# ADR-005: VFS with Capability-Mediated Handles

Status: Accepted
Date: 2026-07-09
Supersedes: —

## Context

Industrial firmware needs to mount different filesystems
(RAMFS in development, LittleFS for the field, FAT32 for
legacy media) without changing user code. The VFS also has
to play well with our capability model: a task that loses
its `Cap<MountPoint>` should immediately lose access to every
file under it.

## Decision

We adopt a **VFS layer** (`caplet-fs`) where every open file
returns a `CapletHandle` that pairs a `Cap<MountPoint>` with a
per-file cookie. The capability is the only durable identity
— closing the file revokes the cookie, not the cap. All
filesystem backends implement a common `FsDriver` trait.

The standard boot layout mounts three RAMFS roots:

```
/tmp    scratch space
/data   persistent application data
/log    ring buffer of audit log lines
```

via `standard_mount_table()` in `caplet-fs::boot`.

## Consequences

- Users can swap backends without changing application code.
- Capability revocation cascades: dropping the `Cap<MountPoint>`
  invalidates every outstanding file handle derived from it.
- Backends are pluggable. RAMFS is the development default; the LittleFS v2
  backend uses a real serialized image through `littlefs-rust`, and the FAT32
  backend parses BPB/FAT chains and 8.3/LFN directory entries. Both persistent
  backends are RAM-image backed; board flash integration supplies the durable
  block I/O layer.

## Alternatives considered

- **Single global fs object**: rejected — does not allow
  per-task mount scoping.
- **Linux-style path-based ACLs**: rejected — we already have
  capabilities, layering ACLs on top is redundant.
