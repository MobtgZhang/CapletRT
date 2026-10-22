# `caplet-hal-nrf52`

nRF52 implementation of the [`caplet-hal`](../caplet-hal) traits for
nRF52832/nRF52840-class parts. Like the STM32 HAL, it is PAC-free and
uses the raw MMIO layout in `raw::nrf52832`.

## Modules

| Module | Public type | Purpose |
|--------|-------------|---------|
| `uart` | `Uarte`, `UartPins` | UARTE0/UARTE1 with EasyDMA-style TX/RX register flow |
| `gpio` | `Pin`, `Port`, `PinRef` | GPIO mode, set/clear/toggle and input |
| `spim` | `Spim` | SPI master (SPIM0..3) |
| `twim` | `Twim` | I2C/TWI master (TWIM0..3) |
| `timer` | `Timer` | TIMER0..4 one-shot/periodic operation |
| `rtc` | `Rtc` | RTC1 tick source |
| `irq` | `IrqController` | NVIC facade |

## Example

```rust,no_run
use caplet_hal::uart::{Uart, UartConfig};
use caplet_hal_nrf52::Uarte;

// Safety: the board owns UARTE0 and has not routed these pins elsewhere.
let mut uart = unsafe { Uarte::uarte0() };
uart.set_pins(6, 8, None, None)?;
uart.configure(UartConfig::default())?;
uart.write(b"CapletRT\r\n")?;
# Ok::<(), caplet_hal::HalError>(())
```

## Pinout

The crate does not hard-code a development-kit pinout. The board passes
`UartPins`/`SpiPins`/`I2cPins` explicitly; on an nRF52840 DK the common
UARTE0 assignment is TXD=P0.06, RXD=P0.08, RTS=P0.05, CTS=P0.07. Verify
the assignment against the schematic before enabling a peripheral.

## Safety

Constructors are `unsafe` and transfer exclusive ownership of the raw
peripheral. Register access is volatile MMIO. The caller must also keep
pin assignments disjoint across UARTE/SPIM/TWIM instances.

## Limitations

- No nRF52 board or QEMU/Renode model is shipped in this repository, so
  the crate is host-compiled only and has no emulator smoke test.
- EasyDMA buffer descriptors are programmed by the driver, but cache
  maintenance for SoftDevice-style shared RAM is out of scope.
- No Bluetooth/SoftDevice integration.
