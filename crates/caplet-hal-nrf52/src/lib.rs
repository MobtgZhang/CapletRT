//! nRF52 HAL implementation
//!
//! Implements the [`caplet_hal`] traits for Nordic nRF52 series
//! microcontrollers (nRF52832 / nRF52840). Uses a PAC-free design
//! where the board is expected to provide the [raw] MMIO layout
//! for the chip variant.

#![no_std]
#![allow(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

pub mod gpio;
pub mod irq;
pub mod raw;
pub mod rtc;
pub mod spim;
pub mod timer;
pub mod twim;
pub mod uart;

pub use gpio::{Pin, Port, PortMode};
pub use irq::IrqController;
pub use rtc::Rtc;
pub use spim::Spim;
pub use timer::Timer as NrfTimer;
pub use twim::Twim;
pub use uart::Uarte;

/// Re-export HAL trait root.
pub use caplet_hal as hal;

/// Common imports for board crates using this HAL.
pub mod prelude {
    pub use crate::hal::{
        gpio::{GpioMode, GpioPin, GpioPort, PinState},
        i2c::{I2c, I2cConfig},
        interrupt::{InterruptPriority, IrqHandler},
        spi::{Spi, SpiConfig},
        timer::Timer as HalTimer,
        uart::{Uart, UartConfig, UartFmt},
    };
    pub use crate::{IrqController, NrfTimer, Pin, Port, PortMode, Rtc, Spim, Twim, Uarte};
}
