# `caplet-cortex-a`

ARMv7-A (Cortex-A) port used by the `hello-cortex-a9` board and the
Allwinner A20 model.

## Contents

- `boot` / `context`: EL1 entry, task context switching.
- `gic`: GICv2 interrupt controller support.
- `mmu`: page-table construction and permission encoding.
- `pl011_uart`: AArch32 PL011 console helpers.
- `smp`: secondary-core bring-up helpers.
- `smc`: secure-monitor call stubs used by the capability tests.
- `tick` / `panic` / `kernel_bridge`: timer, panic, and kernel glue.

## Platforms

- QEMU `vexpress-a9` (Cortex-A9, GICv2).
- Renode `cortex-a9.repl` and `allwinner-a20.repl`.

Run through `tools/sim/run_qemu.sh cortex-a9|a20` and
`tools/sim/run_renode.sh cortex-a9|a20`. The A20 model reproduces the
QEMU-compatible low DDR window; it is not a full Allwinner peripheral
model.
