# Porting CapletRT to a New Board

This guide walks through the steps required to bring up
CapletRT on a new Cortex-M (or Cortex-A) target.

## 1. Toolchain

Add a new target with `rustup`:

```bash
rustup target add thumbv7em-none-eabihf   # Cortex-M4F example
```

Update the workspace `rust-toolchain.toml` if you want the new
target to be the default.

## 2. HAL implementation

The HAL crate for the new board (e.g. `caplet-hal-myboard`)
must implement every trait declared by `caplet-hal`:

| Trait        | Required? | Notes                                  |
|--------------|-----------|----------------------------------------|
| `Uart`       | yes       | Used by `caplet-log::writer::UartWriter`. |
| `GpioPin`    | yes       | LED toggle helper uses it.             |
| `Timer`      | yes       | SysTick abstraction.                   |
| `Interrupt`  | yes       | NVIC on Cortex-M.                      |
| `I2c`        | optional  | Sensors.                               |
| `Spi`        | optional  | Flash, display.                        |
| `Phy`        | optional  | Ethernet.                              |

A typical `lib.rs` looks like:

```rust
pub struct MyBoardUart { /* registers */ }

impl Uart for MyBoardUart {
    type Error = UartError;
    fn write(&mut self, b: u8) -> Result<(), Self::Error> { ... }
    fn read(&mut self) -> Result<u8, Self::Error> { ... }
}
```

Bump the HAL SemVer in `caplet-hal::version_gate` whenever you
change a trait signature (see ADR-later).

## 3. Linker script + memory map

Create `boards/my-board/memory.x` describing FLASH and RAM.
Example (Nucleo-F446ZE):

```
MEMORY {
    FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 512K
    RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 128K
}
```

Add a `build.rs` that copies `memory.x` into `OUT_DIR` and
emits `cargo:rustc-link-arg=-Tmemory.x`.

## 4. Board crate

Create `boards/my-board/Cargo.toml` with the standard deps:

```toml
[dependencies]
caplet = { path = "../crates/caplet-kernel" }
caplet-hal = { path = "../crates/caplet-hal" }
caplet-hal-myboard = { path = "../crates/caplet-hal-myboard" }
cortex-m-rt = "0.7"
```

`src/main.rs` typically contains:

```rust
#[cortex_m_rt::entry]
fn main() -> ! {
    static EXECUTOR: StaticCell<Executor> = ...;
    caplet::kernel_init(EXECUTOR);
    spawn(my_task());
    executor.run()
}
```

## 5. CI integration

Register the board in `tools/sim/common.sh` (`board_spec` and
`all_boards`). QEMU and Renode then run through the shared entry points:

```bash
tools/sim/run_qemu.sh   my-board
tools/sim/run_renode.sh my-board
```

CI invokes `tools/sim/run_matrix.sh` and `tools/sim/renode_matrix.sh`, which
cover every registered board and aggregate failures. The legacy
`tests/qemu-*/run.sh` scripts are thin compatibility wrappers around the
same runner.

## 6. Sanity checks

Run `tools/hal_version_gate.sh` and the `qemu-my-board` matrix. The CI gate
refuses a PR that bumps a HAL trait without updating the SemVer constants.

## Common pitfalls

- **AtomicU64 not available on Cortex-M0**: use the
  `IDLE_TICKS_LO/HI` pattern (ADR-0003).
- **MPU region count** differs between cores (Cortex-M0: 8,
  Cortex-M4: 8, Cortex-M7: 16). Set `KERNEL_CONFIG.mpu_regions`
  accordingly.
- **FPU presence** on Cortex-M4F: enable `target-feature=+fp`
  in `.cargo/config.toml` to gain hardware FP.
