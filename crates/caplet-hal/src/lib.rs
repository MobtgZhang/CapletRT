//! CapletRT Hardware Abstraction Layer
//!
//! This crate defines the HAL traits for CapletRT.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

extern crate alloc;

pub mod uart;
pub mod gpio;
pub mod i2c;
pub mod spi;
pub mod timer;
pub mod interrupt;
pub mod led;
pub mod version_gate;
pub mod phy;

pub use uart::Uart;
pub use gpio::{GpioPin, GpioPort, GpioMode, PinState};
pub use i2c::I2c;
pub use spi::Spi;
pub use timer::Timer;
pub use interrupt::Interrupt;
pub use led::Led;
pub use phy::{Duplex, LinkState, Mac, Phy, PhyError};

/// Error type for HAL operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HalError {
    /// Operation timed out
    Timeout,
    /// Invalid argument
    InvalidArgument,
    /// Hardware error
    HardwareError,
    /// Not ready
    NotReady,
    /// Disabled
    Disabled,
}

impl core::fmt::Display for HalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Timeout => write!(f, "operation timed out"),
            Self::InvalidArgument => write!(f, "invalid argument"),
            Self::HardwareError => write!(f, "hardware error"),
            Self::NotReady => write!(f, "not ready"),
            Self::Disabled => write!(f, "device disabled"),
        }
    }
}

impl From<HalError> for () {
    fn from(_: HalError) {}
}

/// Result type for HAL operations
pub type HalResult<T> = Result<T, HalError>;
