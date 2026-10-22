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

extern crate alloc;

pub mod flash;
pub mod gpio;
pub mod i2c;
pub mod irq;
pub mod phy_dp83848;
pub mod raw;
pub mod rcc;
pub mod spi;
pub mod timer;
pub mod uart;

pub use flash::Flash;
pub use gpio::{Pin, PinRef, Port, PortMode};
pub use i2c::I2cMaster;
pub use irq::IrqController;
pub use phy_dp83848::Dp83848;
pub use rcc::Rcc;
pub use spi::SpiMaster;
pub use timer::StmTimer;
pub use uart::Usart;

/// Re-export of the trait root.
pub use caplet_hal as hal;

/// Public re-exports for use in boards
pub mod prelude {
    pub use crate::hal::{
        gpio::{GpioMode, GpioPin, GpioPort, PinState},
        i2c::{I2c, I2cConfig},
        interrupt::{InterruptPriority, IrqHandler},
        spi::{Spi, SpiConfig},
        timer::Timer as HalTimer,
        uart::{Uart, UartConfig, UartFmt},
    };
    pub use crate::{
        I2cMaster, IrqController, Pin, Port, PortMode, Rcc, SpiMaster, StmTimer, Usart,
    };
}
