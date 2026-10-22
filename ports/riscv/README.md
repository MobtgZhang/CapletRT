# `caplet-riscv`

RISC-V port for RV32IMAC and RV64GC.

## Boot flow

```text
reset (M-mode) -> PMP setup + delegation + mret
               -> S-mode s_mode_entry -> riscv_boot()
               -> BSS clear + UART -> early_init -> board rust_main
```

The kernel runs in S-mode; both QEMU and Renode enter the ELF in M-mode.

## Contents

- `arch` / `boot` / `context`: privilege transition and task switching.
- `clint` / `plic`: timer and interrupt controllers.
- `mmu`: Sv32/Sv39 page tables.
- `uart`: 16550-compatible console.
- `tick` / `log` / `panic` / `kernel_bridge`: timer, console writer,
  panic, and kernel glue.

## Platforms

- QEMU `virt` (`qemu-system-riscv32` / `qemu-system-riscv64`).
- Renode `riscv_virt` (`rv32.repl` / `rv64.repl`).

Memory map: DRAM at `0x8000_0000`, CLINT at `0x0200_0000`, PLIC at
`0x0c00_0000`, UART0 at `0x1000_0000`.
