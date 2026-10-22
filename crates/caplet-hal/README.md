# `caplet-hal`

The Hardware Abstraction Layer: traits every peripheral must
implement. Concrete implementations live in `caplet-hal-stm32f4`,
`caplet-hal-nrf52`, `caplet-hal-gd32`, etc.

## Modules

| Module            | Trait(s)                                            |
|-------------------|-----------------------------------------------------|
| `uart`            | `Uart`, `UartFmt`.                                  |
| `gpio`            | `GpioPin`, `GpioPort`, `GpioMode`, `PinState`.      |
| `i2c`             | `I2c`.                                              |
| `spi`             | `Spi`.                                              |
| `timer`           | `Timer`.                                            |
| `interrupt`       | `Interrupt`.                                        |
| `led`             | `Led` (LED toggle helper).                          |
| `phy`             | `Phy`, `LinkState`, `Duplex`, `PhyError`.           |
| `version_gate`    | HAL SemVer constants and CI gate.                   |

## SemVer policy

`version_gate.rs` exposes `HAL_VERSION_MAJOR/MINOR/PATCH`.
The CI gate script `tools/hal_version_gate.sh` refuses any
PR that changes a HAL trait signature without bumping the
major or minor version.

## Stability

Breaking changes are allowed within a major version line;
they trigger the gate.