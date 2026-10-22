# `nucleo-f446ze`

Nucleo-F446ZE board crate (STM32F446ZE, Cortex-M4F) using the
`caplet-hal-stm32f4` peripheral implementation.

## Behaviour

The board boots with a semihosting log writer, initialises the capability
kernel, and runs two async tasks:

- an LED task toggling PB0 through `GPIOB_BSRR`,
- a monitor task reporting scheduler progress.

It prints:

```text
Nucleo-F446ZE boot OK
```

## Memory map

| Region | Origin       | Size   |
|--------|--------------|--------|
| FLASH  | `0x0800_0000` | 512 KiB |
| RAM    | `0x2000_0000` | 128 KiB |

The linker reserves an 8 KiB stack and a 16 KiB heap (`memory.x`).

## Build and run

```bash
tools/sim/run_qemu.sh nucleo
tools/sim/run_renode.sh nucleo
```

The QEMU path uses the `olimex-stm32-h405` machine with semihosting; the
Renode path uses `renode/nucleo-f446ze.repl` with the emulated UART. These
are board-model approximations, not a substitute for hardware validation.
