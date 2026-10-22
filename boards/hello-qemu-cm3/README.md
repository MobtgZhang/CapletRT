# `hello-qemu-cm3`

Minimal Cortex-M3 boot demo for the QEMU `mps2-an385` machine and the
matching Renode MPS2-AN385 platform.

## Behaviour

The board initialises the kernel and prints:

```text
hello world
```

Because Cortex-M3 lacks atomic CAS, the demo does not run the stackless
async executor; it drives the kernel directly and then idles.

## Memory map

The MPS2-AN385 model provides:

- code: 4 MiB at `0x0000_0000`
- SRAM: 4 MiB at `0x2000_0000`

## Build and run

```bash
tools/sim/run_qemu.sh cm3
tools/sim/run_renode.sh cm3
```

QEMU machine: `mps2-an385`. Renode platform:
`renode/mps2-an385.repl`.
