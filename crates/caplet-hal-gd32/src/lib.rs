//! GD32 HAL — PLACEHOLDER
//!
//! GigaDevice GD32F303 / GD32F450 microcontrollers share most of
//! their register layout with STM32F1 / STM32F4 respectively.
//! For M2 we expose:
//!
//! * [`reexport_stm_f4`] — type alias bundle pointing at the STM32F4
//!   HAL, suitable for board crates that target GD32F450 (which
//!   is a drop-in replacement for STM32F4). This is the recommended
//!   path for early adopters because no extra code has to be
//!   audited.
//! * [`Gd32Family`] — enum marker the kernel can match on when
//!   selecting peripheral workarounds (e.g. GD32 errata).
//!
//! Future M2.x releases will introduce first-class drivers for
//! GD32-specific peripherals (the high-resolution TIMER0, the
//! EXMC memory controller, the SDIO host).

#![no_std]
#![deny(missing_docs)]

pub use caplet_hal_stm32f4 as f4_compat;

/// GD32 chip family marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gd32Family {
    /// GD32F303 — STM32F1-compatible Cortex-M4 at 120 MHz.
    F303,
    /// GD32F450 — STM32F429-compatible Cortex-M4 at 200 MHz.
    F450,
    /// GD32VF103 — RISC-V (experimental).
    Vf103,
}

/// Compile-time chip family selection.
pub const FAMILY: Gd32Family = Gd32Family::F450;

/// Type alias bundle for `GD32F4`-compatible boards. Boards
/// targeting the F450 can `use caplet_hal_gd32::reexport_stm_f4::*`
/// to pull in the STM32F4 driver types verbatim.
pub mod reexport_stm_f4 {
    pub use crate::f4_compat::Flash;
    pub use crate::f4_compat::Pin;
    pub use crate::f4_compat::PinRef;
    pub use crate::f4_compat::Port;
    pub use crate::f4_compat::PortMode;
    pub use crate::f4_compat::Rcc;
    pub use crate::f4_compat::StmTimer;
    pub use crate::f4_compat::Usart;
}

/// GD32-specific errata that the kernel should work around.
///
/// Some errata entries are documented but not yet implemented;
/// they appear in the module so that downstream board code can
/// `match` on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Errata {
    /// On GD32F303 rev. A: SPI0 loses bytes above 36 MHz. Use SPI1/2.
    Spi0HighSpeed,
    /// On all GD32F450 revs: ADC needs an extra warmup cycle.
    AdcNeedsWarmup,
    /// On GD32VF103: PFIC does not support nested interrupts.
    NoNestedIrq,
}

const ERRATA_LIST: &[Errata] = &[Errata::AdcNeedsWarmup, Errata::NoNestedIrq];

/// Apply the static errata workarounds that affect the currently
/// selected `FAMILY`. Idempotent; safe to call multiple times.
///
/// Returns the list of applied workarounds.
pub fn apply_errata() -> &'static [Errata] {
    // No actual workarounds for the placeholder; the list is
    // returned for logging.
    ERRATA_LIST
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_is_f450() {
        assert_eq!(FAMILY, Gd32Family::F450);
    }

    #[test]
    fn apply_errata_returns_list() {
        let list = apply_errata();
        assert!(!list.is_empty());
        assert!(list.contains(&Errata::AdcNeedsWarmup));
    }
}
