# `caplet-cortex-m`

Architecture port for ARM Cortex-M (M3/M4/M7 class) targets.

## Contents

| Module          | Purpose                                                        |
|-----------------|----------------------------------------------------------------|
| `boot`          | `cortex_m_boot` entry glue and early init.                     |
| `context`       | PSP-based task context switch, PendSV hook, SVC hook.          |
| `interrupt`     | Exception/interrupt frames and handler registration.          |
| `systick`       | SysTick programming and tick accounting.                       |
| `tickless`      | Tickless idle and WFI helpers.                                 |
| `qemu_uart`     | UART driver for the MPS2 QEMU models.                          |
| `panic`         | Panic handler (feature-gated, excluded under `cfg(test)`).     |
| `kernel_bridge` | Wiring from the port to `caplet-kernel`.                       |

## Targets

- `thumbv7m-none-eabi` for Cortex-M3 boards.
- `thumbv7em-none-eabihf` for Cortex-M4F boards.

Cortex-M3 has no general atomic compare-and-swap; boards that need the
stackless executor must either enable a critical-section based lock or
drive the kernel directly, as `hello-qemu-cm3` does.

## Testing

The port is validated by the board matrix, not by host unit tests:

```bash
tools/sim/run_qemu.sh cm3
tools/sim/run_qemu.sh cm4
tools/sim/run_renode.sh cm4
```

Targets, expected markers, and QEMU/Renode machine names come from
`tools/sim/common.sh`.
