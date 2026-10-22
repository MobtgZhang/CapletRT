# `caplet-aarch64`

AArch64 (ARM64) port targeting EL1 with the MMU initially disabled.

## Contents

- `boot`: EL1 entry and stack setup.
- `context`: task context save/restore.
- `gic`: GICv3 support.
- `mmu`: AArch64 translation tables and permission bits.
- `tick`: generic timer tick source.
- `uart`: PL011 console.
- `panic` / `kernel_bridge`: panic path and kernel glue.

## Platforms

- QEMU `virt` with `-cpu cortex-a53`.
- Renode Cortex-A53/GICv3 platform.

Memory map (128 MiB): PL011 at `0x0900_0000`, GICv3 distributor at
`0x0800_0000`, DRAM at `0x4000_0000`.

Run with `tools/sim/run_qemu.sh aarch64` or
`tools/sim/run_renode.sh aarch64`.
