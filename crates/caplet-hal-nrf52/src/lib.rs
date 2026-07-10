//! nRF52 HAL implementation
//!
//! Implements the [`caplet_hal`] traits for Nordic nRF52 series
//! microcontrollers (nRF52832 / nRF52840). Uses a PAC-free design
//! where the board is expected to provide the [raw] MMIO layout
//! for the chip variant.

#![no_std]
#![allow(unsafe_code)]
#![deny(missing_docs)]

pub mod raw;
pub mod uart;
pub mod gpio;
pub mod timer;
pub mod rtc;

pub use uart::Uarte;
pub use gpio::{Pin, Port, PortMode};
pub use timer::Timer as NrfTimer;
pub use rtc::Rtc;

/// Re-export HAL trait root.
pub use caplet_hal as hal;

/// Common imports for board crates using this HAL.
pub mod prelude {
    pub use crate::hal::{
        gpio::{GpioMode, GpioPin, GpioPort, PinState},
        uart::{Uart, UartConfig, UartFmt},
        timer::Timer as HalTimer,
    };
    pub use crate::{NrfTimer, Pin, Port, PortMode, Rtc, Uarte};
}