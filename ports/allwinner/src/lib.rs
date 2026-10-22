#![no_std]
//! Allwinner (sunxi) port.
//!
//! The A20 is an ARMv7-A (Cortex-A8/A7) SoC. `caplet-cortex-a` provides
//! the architecture-level boot, context-switch and MMU support; this
//! crate adds the sunxi-specific console.

#[cfg(feature = "cortex-a")]
pub use caplet_cortex_a::*;

pub mod uart;

pub use uart::{Ns16550Uart, UART0_BASE};
