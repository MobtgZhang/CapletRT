//! DP83848 Ethernet PHY driver — register-level model.
//!
//! The DP83848 is a National Instruments 10/100 Mbit PHY
//! commonly paired with STM32 ETH MACs. This module models
//! the register layout and exposes a [`Dp83848`] state holder
//! that the platform's MDIO bus implementation drives.

use caplet_hal::phy::{Duplex, LinkState, Phy, PhyError};

/// PHY identifier (OUI + model) for DP83848.
pub const DP83848_PHY_ID: u32 = 0x2000_5C90;

/// DP83848 register map.
pub mod reg {
    /// Basic Mode Control Register.
    pub const BMCR: u8 = 0x00;
    /// Basic Mode Status Register.
    pub const BMSR: u8 = 0x01;
    /// PHY Identifier 1.
    pub const PHYID1: u8 = 0x02;
    /// PHY Identifier 2.
    pub const PHYID2: u8 = 0x03;
    /// Auto-Negotiation Advertisement.
    pub const ANAR: u8 = 0x04;
    /// Auto-Negotiation Link Partner Ability.
    pub const ANLPAR: u8 = 0x05;
    /// PHY Status.
    pub const PHYSTS: u8 = 0x10;
}

/// BMCR bits.
pub mod bmcr {
    /// Reset.
    pub const RESET: u16 = 1 << 15;
    /// Loopback.
    pub const LOOPBACK: u16 = 1 << 14;
    /// Speed selection LSB.
    pub const SPEED_LSB: u16 = 1 << 13;
    /// Auto-negotiation enable.
    pub const AN_ENABLE: u16 = 1 << 12;
    /// Power down.
    pub const POWER_DOWN: u16 = 1 << 11;
    /// Isolate.
    pub const ISOLATE: u16 = 1 << 10;
    /// Restart auto-negotiation.
    pub const RESTART_AN: u16 = 1 << 9;
    /// Duplex mode.
    pub const DUPLEX: u16 = 1 << 8;
    /// Speed selection MSB.
    pub const SPEED_MSB: u16 = 1 << 6;
}

/// BMSR bits.
pub mod bmsr {
    /// 100BASE-T4 capable.
    pub const BASE_T4: u16 = 1 << 15;
    /// 100BASE-TX full duplex.
    pub const BASE_TX_FD: u16 = 1 << 14;
    /// 100BASE-TX half duplex.
    pub const BASE_TX_HD: u16 = 1 << 13;
    /// 10BASE-T full duplex.
    pub const BASE_T_FD: u16 = 1 << 12;
    /// 10BASE-T half duplex.
    pub const BASE_T_HD: u16 = 1 << 11;
    /// Auto-negotiation complete.
    pub const AN_COMPLETE: u16 = 1 << 5;
    /// Link up.
    pub const LINK_STATUS: u16 = 1 << 2;
}

/// A DP83848 PHY. `regs` is a small 32-entry register file
/// that simulates the device for tests and for boards that
/// drive the PHY through a memory-mapped MDIO bus.
#[derive(Debug, Clone)]
pub struct Dp83848 {
    /// Register file. Index by `reg::*`.
    pub regs: core::cell::Cell<[u16; 32]>,
    /// PHY address on the MDIO bus.
    pub address: u8,
}

impl Default for Dp83848 {
    fn default() -> Self {
        Self::new()
    }
}

impl Dp83848 {
    /// Construct a fresh DP83848 with sensible defaults
    /// (advertise 10/100 FD/HD, AN enabled, link up at 100 FD).
    pub fn new() -> Self {
        let mut regs = [0u16; 32];
        regs[reg::PHYID1 as usize] = (DP83848_PHY_ID >> 16) as u16;
        regs[reg::PHYID2 as usize] = (DP83848_PHY_ID & 0xFFFF) as u16;
        regs[reg::BMCR as usize] = bmcr::AN_ENABLE | bmcr::DUPLEX | bmcr::SPEED_LSB;
        regs[reg::BMSR as usize] = bmsr::BASE_TX_FD
            | bmsr::BASE_TX_HD
            | bmsr::BASE_T_FD
            | bmsr::BASE_T_HD
            | bmsr::AN_COMPLETE
            | bmsr::LINK_STATUS;
        regs[reg::ANAR as usize] = 0x01E1; // advertise 10/100 FD/HD
        regs[reg::ANLPAR as usize] = 0x01E1; // peer agrees
        regs[reg::PHYSTS as usize] = (1 << 2) | (1 << 1); // link + speed100 + FD
        Self {
            regs: core::cell::Cell::new(regs),
            address: 0,
        }
    }
}

impl Phy for Dp83848 {
    fn reset(&self) -> Result<(), PhyError> {
        // We just acknowledge the reset; the caller will see
        // BMCR.RESET cleared in subsequent reads.
        Ok(())
    }

    fn read(&self, reg: u8) -> Result<u16, PhyError> {
        if (reg as usize) >= self.regs.get().len() {
            return Err(PhyError::Bus);
        }
        Ok(self.regs.get()[reg as usize])
    }

    fn write(&self, reg: u8, value: u16) -> Result<(), PhyError> {
        if (reg as usize) >= self.regs.get().len() {
            return Err(PhyError::Bus);
        }
        let mut copy = self.regs.get();
        copy[reg as usize] = value;
        self.regs.set(copy);
        Ok(())
    }

    fn link_state(&self) -> Result<LinkState, PhyError> {
        let bmsr = self.read(reg::BMSR)?;
        if bmsr & bmsr::LINK_STATUS == 0 {
            return Ok(LinkState::Down);
        }
        let phys = self.read(reg::PHYSTS)?;
        // PHYSTS bit 1: speed (0=10, 1=100). Bit 2: duplex.
        if phys & (1 << 1) != 0 {
            Ok(LinkState::Up100)
        } else {
            Ok(LinkState::Up10)
        }
    }

    fn duplex(&self) -> Result<Duplex, PhyError> {
        let phys = self.read(reg::PHYSTS)?;
        if phys & (1 << 2) != 0 {
            Ok(Duplex::Full)
        } else {
            Ok(Duplex::Half)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_then_read() {
        let p = Dp83848::new();
        assert_eq!(p.reset(), Ok(()));
        let bmcr = p.read(reg::BMCR).unwrap();
        assert_ne!(bmcr & bmcr::AN_ENABLE, 0);
    }

    #[test]
    fn link_state_default_up100() {
        let p = Dp83848::new();
        assert_eq!(p.link_state(), Ok(LinkState::Up100));
        assert_eq!(p.duplex(), Ok(Duplex::Full));
    }

    #[test]
    fn link_state_down_when_bmsr_clear() {
        let p = Dp83848::new();
        let mut regs = p.regs.get();
        regs[reg::BMSR as usize] &= !bmsr::LINK_STATUS;
        p.regs.set(regs);
        assert_eq!(p.link_state(), Ok(LinkState::Down));
    }
}
