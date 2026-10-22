# CapletRT simulation matrix

All board launches go through two scripts so QEMU and Renode cannot
drift apart:

```bash
tools/sim/run_qemu.sh riscv64
tools/sim/run_renode.sh riscv64
tools/sim/run_qemu.sh aarch64
tools/sim/renode_matrix.sh
tools/sim/run_matrix.sh
```

Supported board keys:

`riscv32`, `riscv64`, `aarch64`, `cortex-a9`, `a20`, `cm3`, `cm4`,
`cm4-fs`, `nucleo-qemu`, `nucleo`, `industrial-demo`, `energy-meter`,
`medical-device`, `automotive`, plus the eight RISC-V industrial variants:

`industrial-demo-riscv32`, `industrial-demo-riscv64`,
`energy-meter-riscv32`, `energy-meter-riscv64`,
`medical-device-riscv32`, `medical-device-riscv64`,
`automotive-riscv32`, `automotive-riscv64`.

That is 22 board/variant entries. `run_matrix.sh` and `renode_matrix.sh` run
all 22 by default; pass explicit keys to run a subset.

Each run builds the explicit target triple, runs with `timeout`, captures
the full transcript under `target/sim/<board>/`, and asserts the board's
boot marker plus the demo-specific completion lines with `grep`. Renode
command errors are detected separately
because Renode itself exits with status 0 for many monitor failures.

Architectures covered:

- RISC-V 32/64: `qemu-system-riscv32`/`riscv64 -machine virt`, plus Renode
  `riscv-virt` platform descriptions.
- AArch64: `qemu-system-aarch64 -machine virt -cpu cortex-a53`, Renode
  Cortex-A53/GICv3.
- ARMv7-A: QEMU `vexpress-a9`/`cubieboard`, Renode Cortex-A9 and Allwinner A20.
- ARMv7-M/ARMv7E-M: QEMU `mps2-an385`/`mps2-an386` and
  `olimex-stm32-h405`, Renode MPS2/STM32F4 platforms.

## Debugging

`tools/sim/gdb_debug.sh` starts a gdbstub and refuses to run unless a GDB
that supports the board architecture is installed.  Install
`gdb-multiarch` on Debian/Ubuntu, or point `GDB` at an architecture
specific GDB:

```bash
tools/sim/gdb_debug.sh riscv64
tools/sim/gdb_debug.sh aarch64 --renode
GDB=aarch64-linux-gnu-gdb tools/sim/gdb_debug.sh aarch64
```

This is intentional: an x86-only `gdb` is not accepted as proof that a
RISC-V or AArch64 debugging session works.
