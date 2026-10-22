# Porting Guide

The maintained, step-by-step guide is `docs/porting.md`. This page is the
short entry point and records the paths that current CI actually uses.

## Current flow

1. Add the Rust target and a HAL implementation for the new board.
2. Provide `memory.x` and let `cortex-m-rt` (or the RISC-V/AArch64 port)
   own the linker script and vector table.
3. Add the board crate and a simulator definition under the board directory.
4. Register the board in `tools/sim/common.sh::board_spec` (and
   `all_boards`), so QEMU, Renode, GDB, and CI share one definition.
5. Run the board directly, then in both simulators:

   ```bash
   tools/sim/run_qemu.sh <board>
   tools/sim/run_renode.sh <board>
   SIM_TIMEOUT=10 bash tools/sim/run_matrix.sh
   SIM_TIMEOUT=60 bash tools/sim/renode_matrix.sh
   ```

6. Check the HAL contract with `tools/hal_version_gate.sh` (not the old
   `tools/check_hal_version.sh` name).

## Board prerequisites

- A boot marker printed over the emulated console; the simulator scripts
  grep for it and time out otherwise.
- A QEMU machine and/or a Renode `.repl` that matches the memory map.
- The package name, target triple, features, and expected marker recorded in
  `board_spec`, never duplicated in ad-hoc scripts.

## Common pitfalls

- **RAM sizing**: the MPS2 models expose a specific SRAM window; heap bases
  in board `main.rs` must stay inside it.
- **Atomics on Cortex-M0/M3**: `AtomicU64` is unavailable; use the split
  counter pattern or a critical section.
- **MPU regions**: core-dependent counts; set `KERNEL_CONFIG.mpu_regions`.
- **Symbols**: release builds must not strip symbols; GDB smoke tests load
  the ELF and break on `rust_main`/`boot_main`.
- **Semihosting**: QEMU runs with semihosting in some boards, Renode uses
  the emulated UART. Verify the marker appears on the path the script reads.

The full trait table, memory-map examples, and capability expectations are
in `docs/porting.md`.
