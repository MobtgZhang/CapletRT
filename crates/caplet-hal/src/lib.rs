//! CapletRT Hardware Abstraction Layer
//!
//! This crate defines the HAL traits for CapletRT.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

extern crate alloc;

pub mod gpio;
pub mod i2c;
pub mod interrupt;
pub mod led;
pub mod phy;
pub mod spi;
pub mod timer;
pub mod uart;
pub mod version_gate;

pub use gpio::{GpioMode, GpioPin, GpioPort, IrqPin, OutputType, PinState, PullMode, Speed};
pub use i2c::{I2c, I2cAddrMode, I2cConfig, I2cError, I2cSpeed};
pub use interrupt::{Interrupt, InterruptPriority, InterruptTrigger, IrqHandler};
pub use led::Led;
pub use phy::{Duplex, LinkState, Mac, Phy, PhyError};
pub use spi::{BitOrder, DataSize, Spi, SpiConfig, SpiMode, SpiModeId};
pub use timer::{Timer, TimerChannel, TimerConfig, TimerMode, TimerRunMode};
pub use uart::{Parity, StopBits, Uart, UartConfig, UartError};

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
    /// UART receiver overrun
    Overrun,
    /// UART framing error
    Framing,
    /// UART parity error
    Parity,
    /// I2C bus is busy
    BusBusy,
    /// I2C arbitration was lost
    ArbitrationLost,
    /// I2C target did not acknowledge
    Nack,
    /// The requested operation is not implemented by this driver
    NotSupported,
}

impl core::fmt::Display for HalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Timeout => write!(f, "operation timed out"),
            Self::InvalidArgument => write!(f, "invalid argument"),
            Self::HardwareError => write!(f, "hardware error"),
            Self::NotReady => write!(f, "not ready"),
            Self::Disabled => write!(f, "device disabled"),
            Self::Overrun => write!(f, "receiver overrun"),
            Self::Framing => write!(f, "framing error"),
            Self::Parity => write!(f, "parity error"),
            Self::BusBusy => write!(f, "bus busy"),
            Self::ArbitrationLost => write!(f, "arbitration lost"),
            Self::Nack => write!(f, "negative acknowledgement"),
            Self::NotSupported => write!(f, "operation not supported"),
        }
    }
}

impl From<HalError> for caplet_cap::error::CapError {
    fn from(error: HalError) -> Self {
        use caplet_cap::error::CapError;

        match error {
            HalError::Timeout => CapError::Timeout,
            HalError::InvalidArgument => CapError::Invalid,
            HalError::HardwareError
            | HalError::Overrun
            | HalError::Framing
            | HalError::Parity
            | HalError::BusBusy
            | HalError::ArbitrationLost
            | HalError::Nack => CapError::Hardware,
            HalError::NotReady | HalError::Disabled | HalError::NotSupported => {
                CapError::Unavailable
            }
        }
    }
}

impl From<HalError> for () {
    fn from(_: HalError) {}
}

/// Result type for HAL operations
pub type HalResult<T> = Result<T, HalError>;
