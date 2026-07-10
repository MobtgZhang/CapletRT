//! PMP (Physical Memory Protection) for RISC-V.
//!
//! The PMP provides hardware-enforced memory protection on RISC-V
//! cores that lack an MMU. It is configured in `arch::configure_pmp()`
//! during early boot.
//!
//! The port uses PMP to enforce:
//!   - Kernel code + data: read + write + execute (0x8000_0000+)
//!   - Device MMIO regions: read + write (no execute)
//!   - Everything else: no access (locked)
//!
//! This module is currently a placeholder documenting the design.
//! The actual PMP configuration is done inline in `arch.rs::configure_pmp()`.

/// Number of PMP entries available (minimum 16 on most cores).
pub const PMP_ENTRY_COUNT: usize = 16;

/// PMP address register encoding mode.
#[derive(Debug, Clone, Copy)]
pub enum PmpAddrMode {
    /// TOR — top-of-range (address in pmpaddrN-1).
    Tor,
    /// NAPOT — naturally aligned power-of-two.
    Napot,
    /// NA4 — naturally aligned 4-byte region.
    Na4,
}

/// PMP configuration register entry.
#[derive(Debug, Clone, Copy)]
pub struct PmpCfg {
    /// Which PMP entry this config applies to.
    pub entry: u8,
    /// Read permission.
    pub read: bool,
    /// Write permission.
    pub write: bool,
    /// Execute permission.
    pub exec: bool,
    /// Locked (persists after mret, ignore on W chapters).
    pub lock: bool,
    /// Address mode.
    pub mode: PmpAddrMode,
}

impl PmpCfg {
    /// Kernel code region: RWX, locked.
    ///
    /// Covers the entire QEMU virt DRAM at 0x8000_0000.
    pub fn kernel_code() -> Self {
        Self {
            entry: 0,
            read: true,
            write: true,
            exec: true,
            lock: true,
            mode: PmpAddrMode::Napot,
        }
    }

    /// Device MMIO region: RW, locked.
    pub fn mmio(base: u32, size: u32) -> Self {
        let _ = base;
        let _ = size;
        Self {
            entry: 1,
            read: true,
            write: true,
            exec: false,
            lock: true,
            mode: PmpAddrMode::Tor,
        }
    }
}
