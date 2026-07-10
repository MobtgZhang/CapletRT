# FAT32 image fixtures

This directory holds pre-generated FAT32 filesystem images used
by the `caplet-fs` test harness. The images are committed to
the repository so CI runs are reproducible.

| File                  | Size   | Purpose                                                |
|-----------------------|--------|--------------------------------------------------------|
| `sample.fat32`        | 256 KB | Empty FAT32 volume, 512 byte sectors, 1 sector/cluster |

## Regenerating

The stub images can be regenerated with `mkfs.fat`:

```bash
dd if=/dev/zero of=sample.fat32 bs=1024 count=256
mkfs.fat -F 32 -s 1 -S 512 -n CAPLET sample.fat32
```

Once the real FAT32 driver is implemented the tests will mount
this image, write a sentinel file, unmount, re-mount and verify
the file is intact.