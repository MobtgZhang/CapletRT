# CapletRT Project Status

Last verified: 2026-10-22 on the local Linux host.

## Verification matrix

| Area | Command | Result |
|------|---------|--------|
| Host tests | `cargo test --workspace -- --nocapture` | pass |
| Formatting | `cargo fmt --all -- --check` | pass |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| QEMU boards | `SIM_TIMEOUT=10 bash tools/sim/run_matrix.sh` | `PASS: QEMU matrix (22 boards)` |
| Renode boards | `SIM_TIMEOUT=60 bash tools/sim/renode_matrix.sh` | `PASS: Renode matrix (22 boards)` |
| Kani | `bash tools/kani/run_catalogue.sh` | see `docs/kani.md`; 39 proofs |
| GDB | `tools/sim/gdb_debug.sh` | RISC-V64, AArch64, ARMv7-A breakpoints hit |

## Board matrix

QEMU and Renode share one definition in `tools/sim/common.sh`
(`board_spec` + `all_boards`), so both backends build the same ELF, use the
same memory map, and assert the same completion marker.

| Group | Keys |
|-------|------|
| RISC-V | `riscv32`, `riscv64` |
| AArch64 | `aarch64` |
| Cortex-A | `cortex-a9`, `a20` |
| Cortex-M | `cm3`, `cm4`, `cm4-fs` |
| STM32F4 | `nucleo-qemu`, `nucleo` |
| Industrial (Cortex-M4F) | `industrial-demo`, `energy-meter`, `medical-device`, `automotive` |
| Industrial (RISC-V32) | `industrial-demo-riscv32`, `energy-meter-riscv32`, `medical-device-riscv32`, `automotive-riscv32` |
| Industrial (RISC-V64) | `industrial-demo-riscv64`, `energy-meter-riscv64`, `medical-device-riscv64`, `automotive-riscv64` |

Both matrices were run serially on this host: 22 passes, 0 failures, exit
code 0 for QEMU and Renode.

## Renode integration

- Platform descriptions live under each board's `renode/` directory.
- `tools/sim/run_renode.sh` loads the `.repl`, loads the ELF, captures the
  console, rejects monitor errors/unhandled accesses, and asserts the same
  marker as QEMU.
- Cortex-M4 uses the `mps2-an500.repl` model in Renode; QEMU uses
  `mps2-an386`. They are both Cortex-M4 but have different peripheral models.
- Cortex-A9, Allwinner A20, Cortex-A53, RV32, and RV64 each have their own
  Renode platform. These are simulation models, not silicon timing.

## Debugging

`tools/sim/gdb_debug.sh <board> [--renode]` probes the installed GDB with
`set architecture` before using it. On this host the distribution `gdb`
only supports the native architecture, so `gdb-multiarch` was used.

Verified breakpoints:

| Architecture | Symbol | Evidence |
|--------------|--------|----------|
| RISC-V64 | `rust_main` | breakpoint hit; backtrace reaches `riscv_boot` / `s_mode_entry` |
| AArch64 | `rust_main` | breakpoint hit; backtrace reaches `aarch64_boot` / `_start` |
| ARMv7-A | `boot_main` | breakpoint hit; backtrace reaches `_reset` |

## Known limits

- No physical board was exercised in this verification; all results are
  QEMU/Renode models.
- The industrial demo stores records in RAM (`[ram-backed]`). The MPS2 model
  has no persistent flash controller, so power-loss persistence is not
  claimed.
- The MQTT-SN E2E uses an in-process `MockBroker` that parses the real wire
  bytes; it is not a live `rumqttd` broker and does not use a host TCP stack.
- There is no QEMU slirp TCP echo board test yet.
- `hello-dynamic-loader` is host-side ELF/.capp E2E, not boot-ROM loading.
- OTA A/B, hashing, signature, and rollback have host E2E coverage, but no
  real-flash power-cut injection.
- Kani covers selected capability, syscall, MMU, scheduler, IPC, and protocol
  state machines, not the whole kernel.
- The performance gates in the original M2 plan were not timed on
  `mps2-an500`.

## Reproduce

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -- --nocapture
SIM_TIMEOUT=10 bash tools/sim/run_matrix.sh
SIM_TIMEOUT=60 bash tools/sim/renode_matrix.sh
bash tools/kani/run_catalogue.sh
```

See `docs/simulation.md` for the full board table, assertion policy, and GDB
usage.
