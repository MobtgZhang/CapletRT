# Renode Verification Summary

Last verified: 2026-10-22.

## Status: 22/22 boards pass

`SIM_TIMEOUT=60 bash tools/sim/renode_matrix.sh` finishes with:

```text
PASS: Renode matrix (22 boards)
```

The run had 22 `PASS [...] Renode:` lines, zero `FAIL` lines, and exit code
0. The matrix was executed serially to keep memory pressure bounded.

## Entry point

There is one maintained runner:

```bash
tools/sim/run_renode.sh <board-key>
SIM_TIMEOUT=60 bash tools/sim/renode_matrix.sh
```

Board definitions (package, target triple, features, `.repl`, `.resc`, and
expected marker) come from `tools/sim/common.sh`. The old per-board
`tests/renode-*/run.sh` scripts are compatibility wrappers, not a second
implementation.

## Platforms

| Architecture | Boards | Renode platform |
|--------------|--------|-----------------|
| RISC-V32 | `riscv32`, `*-riscv32` | `rv32.repl` / `riscv32.repl` |
| RISC-V64 | `riscv64`, `*-riscv64` | `rv64.repl` / `riscv64.repl` |
| AArch64 | `aarch64` | `cortex-a53-gicv3.repl` |
| Cortex-A9 | `cortex-a9` | `cortex-a9.repl` |
| Allwinner A20 | `a20` | `allwinner-a20.repl` |
| Cortex-M3 | `cm3` | `mps2-an385.repl` |
| Cortex-M4F | `cm4`, `cm4-fs`, industrial boards | `mps2-an500.repl` |
| STM32F4 | `nucleo-qemu`, `nucleo` | `stm32f446ze.repl` / `nucleo-f446ze.repl` |

## Assertion policy

`run_renode.sh`:

1. builds the release ELF for the board's target;
2. starts Renode headless with the board `.repl`/`.resc`;
3. captures the full console log under `target/sim/<key>/renode.log`;
4. rejects monitor errors and unhandled accesses;
5. asserts the board's functional completion marker and any extra behaviour
   (task table, MQTT-SN PUBACK, watchdog, alarms, black-box records).

A timeout (124/137) is accepted only after the expected marker has been
observed; a missing marker is always a failure.

## GDB under Renode

```bash
GDB=gdb-multiarch tools/sim/gdb_debug.sh aarch64 --renode
```

Renode starts its GDB server without autostart (`StartGdbServer <port>
false`), so early breakpoints such as `_start` and `rust_main` can be
installed before the machine boots.

## Limits

Renode models are approximations. In particular the Cortex-M4 platform is
named `mps2-an500.repl` while QEMU uses `mps2-an386`; peripheral timing and
the STM32F446ZE model are not silicon-accurate. Results here verify boot,
scheduler, capability, storage, MQTT-SN, and watchdog paths in simulation,
not electrical behaviour on hardware.
