//! STM32F4 HAL implementation
//!
//! Implements the [`caplet_hal`] traits for STM32F4 microcontrollers.
//! All register access is `unsafe` because we read/write raw MMIO
//! pointers. The HAL is split into one file per peripheral so that
//! unused peripherals can be dead-code-eliminated.
//!
//! Address constants come from the STM32F446 reference manual
//! (RM0390). Each peripheral is constructed with a
//! `Peripherals::take()` style singleton — but because we can't
//! depend on PACs in this scaffold, we expose a [`raw`] module with
//! the MMIO register layout that downstream board crates can plug
//! into via [`pac::Peripherals`].

#![no_std]
#![allow(unsafe_code)]
#![deny(missing_docs)]

pub mod raw;
pub mod uart;
pub mod gpio;
pub mod timer;
pub mod rcc;
pub mod flash;
pub mod phy_dp83848;

pub use uart::Usart;
pub use gpio::{Pin, PinRef, Port, PortMode};
pub use timer::StmTimer;
pub use rcc::Rcc;
pub use flash::Flash;
pub use phy_dp83848::Dp83848;

/// Re-export of the trait root.
pub use caplet_hal as hal;

/// Public re-exports for use in boards
pub mod prelude {
    pub use crate::hal::{
        gpio::{GpioMode, GpioPin, GpioPort, PinState},
        uart::{Uart, UartConfig, UartFmt},
        timer::Timer as HalTimer,
    };
    pub use crate::{Pin, Port, PortMode, Rcc, StmTimer, Usart};
}