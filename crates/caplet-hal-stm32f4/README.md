# `caplet-hal-stm32f4`

STM32F4 implementation of the [`caplet-hal`](../caplet-hal) traits.
The crate deliberately has no PAC dependency: each driver owns a raw
MMIO base address from `raw::f446`, so the same code can be used by a
board crate, QEMU, or Renode without pulling in a vendor HAL.

## Modules

| Module | Public type | Purpose |
|--------|-------------|---------|
| `uart` | `Usart` | USART1/USART2 TX/RX and configuration |
| `gpio` | `Pin`, `Port`, `PinRef` | GPIO mode, set/clear/toggle and input |
| `i2c` | `I2cMaster` | I2C1/I2C2 start/stop/restart, ACK/NACK |
| `spi` | `SpiMaster` | SPI1/SPI2/SPI3, mode 0..3 and chip-select |
| `timer` | `StmTimer` | 32-bit TIM2 deadline/periodic operation |
| `irq` | `IrqController` | NVIC enable/disable/priority facade |
| `flash` | `Flash` | Sector erase and word programming |
| `phy_dp83848` | `Dp83848` | DP83848 Ethernet PHY register access |
| `rcc` | `Rcc` | Clock and peripheral clock enables |

## Example

```rust,no_run
use caplet_hal::uart::{Uart, UartConfig};
use caplet_hal_stm32f4::Usart;

// Safety: the board owns USART2 and the APB1 clock is 45 MHz.
let mut uart = unsafe { Usart::usart2(45_000_000) };
uart.configure(UartConfig::default())?;
uart.write(b"CapletRT\r\n")?;
# Ok::<(), caplet_hal::HalError>(())
```

## Board mapping (Nucleo-F446ZE)

| Function | Pins / instance |
|----------|-----------------|
| ST-Link VCP | USART2, PA2/PA3 |
| User LED LD1 | PB0 |
| I2C1 | PB8/PB9 |
| SPI1 | PA5/PA6/PA7 |
| Ethernet RMII | DP83848 on the board's RMII pins |

The STM32F446ZE memory map used by the board is `FLASH=0x0800_0000`
(512 KiB) and `RAM=0x2000_0000` (128 KiB).

## Safety

Constructors are `unsafe`: the caller must guarantee exclusive ownership
of the peripheral and pass the clock frequency that actually feeds it.
All register accesses are volatile MMIO in `raw.rs`. No ISR-safe sharing
is provided by this crate; use `caplet-hal::interrupt` plus the kernel's
capability discipline instead.

## Limitations

- QEMU/Renode model only; no physical-board validation is claimed.
- DMA is not implemented for UART/I2C/SPI. The drivers are polled.
- The F4 flash driver implements sector erase/program, not a wear-leveling
  or power-loss-safe filesystem layer.
