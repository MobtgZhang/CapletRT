//! Ethernet PHY abstraction.
//!
//! A PHY driver is the lowest-level hardware abstraction for
//! the network: it knows about link state, speed, duplex and
//! the management interface (typically MDIO / SMI / SPI).
//! Higher layers (the [`NetworkInterface`](crate::NetworkInterface))
//! talk to a MAC which in turn talks to a PHY.

/// PHY link state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// No carrier.
    Down,
    /// Linked at 10 Mbit/s.
    Up10,
    /// Linked at 100 Mbit/s.
    Up100,
    /// Linked at 1 Gbit/s.
    Up1000,
}

/// PHY duplex mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Duplex {
    /// Half-duplex.
    Half,
    /// Full-duplex.
    Full,
}

/// Errors returned by PHY operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyError {
    /// MDIO bus error (no ack, invalid frame).
    Bus,
    /// PHY did not respond at the requested address.
    NotPresent,
    /// Timed out waiting for an operation.
    Timeout,
    /// Hardware-specific error.
    Hardware,
}

/// Generic PHY trait. Implemented by `Dp83848`, `Lan8720`,
/// `Ip101` etc.
pub trait Phy {
    /// Reset the PHY. Returns when the chip is ready to
    /// accept register reads/writes.
    fn reset(&self) -> Result<(), PhyError>;

    /// Read a register.
    fn read(&self, reg: u8) -> Result<u16, PhyError>;

    /// Write a register.
    fn write(&self, reg: u8, value: u16) -> Result<(), PhyError>;

    /// Current link state.
    fn link_state(&self) -> Result<LinkState, PhyError>;

    /// Current duplex mode.
    fn duplex(&self) -> Result<Duplex, PhyError>;
}

/// MAC address. Stored as six raw bytes; matches the value
/// the MAC block will use as the source address.
pub type Mac = [u8; 6];
