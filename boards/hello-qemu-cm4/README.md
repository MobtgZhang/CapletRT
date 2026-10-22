# `hello-qemu-cm4`

Cortex-M4F boot demo for the QEMU `mps2-an386` machine and the Renode
MPS2-AN500 platform.

## Behaviour

The board prints:

```text
hello world
```

It uses the stackless async executor and a bump allocator over the MPS2
SRAM window.

## Memory map

- code: 4 MiB at `0x0000_0000`
- SRAM: 4 MiB at `0x2000_0000`
- heap used by the demo: 1 MiB at `0x2010_0000`
- stack: 16 KiB (linker-provided)

## Build and run

```bash
tools/sim/run_qemu.sh cm4
tools/sim/run_renode.sh cm4
```

QEMU machine: `mps2-an386`. Renode platform:
`renode/mps2-an500.repl`.
