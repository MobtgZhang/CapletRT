//! Flash-region capability wrapper.
//!
//! Internal flash on Cortex-M microcontrollers is single-owner:
//! writing to it while code is running from it bricks the
//! device. The flash capability token therefore binds two
//! things: proof that the holder may program flash, and a
//! [`FlashRegion`] specifying *which* flash sectors are
//! permitted.

use crate::token::SecurityToken;

/// Errors returned by flash syscalls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashError {
    /// Caller has no [`FlashToken`].
    NoToken,
    /// Region argument is outside the granted range.
    OutOfRegion,
    /// Sector is write-protected by hardware.
    WriteProtected,
    /// Erase/program failed (e.g. VDD drop).
    HardwareError,
    /// Region overlaps an executing code section.
    OverlapsLiveCode,
}

/// Inclusive byte range inside the flash address space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashRegion {
    /// Start address, inclusive.
    pub start: u32,
    /// End address, inclusive.
    pub end: u32,
}

impl FlashRegion {
    /// Construct a new region, returning `None` if `end <
    /// start` (empty region is allowed: `start == end`).
    pub fn new(start: u32, end: u32) -> Option<Self> {
        if end < start {
            None
        } else {
            Some(Self { start, end })
        }
    }

    /// `true` if the byte range `[start..=end]` is fully
    /// contained in `self`.
    pub fn contains(&self, start: u32, end: u32) -> bool {
        start >= self.start && end <= self.end
    }
}

// ── Standard flash partition layout ────────────────────────────────────

/// Flash layout for OTA-capable embedded devices.
///
/// Defines the standard partition layout used across all CapletRT
/// platforms. Partitions are arranged as:
///
/// ```text
/// 0x0000_0000 ┌─────────────────────┐
///              │   Bootloader (32K)  │
/// 0x0000_8000 ├─────────────────────┤
///              │ Primary (A slot)    │
///              │                     │
///              ├─────────────────────┤
///              │ Secondary (B slot)   │
///              │                     │
///              ├─────────────────────┤
///              │  Config (16K)       │
/// 0x00XX_0000 ├─────────────────────┤
///              │  Audit Log (8K)     │
/// 0x00XX_2000 └─────────────────────┘
/// ```
///
/// Specific addresses are filled in by the board's `FlashLayout::new()`
/// based on the SoC's flash geometry.

/// Standard flash partition.
#[derive(Debug, Clone, Copy)]
pub struct FlashLayout {
    /// Bootloader region.
    pub bootloader: FlashRegion,
    /// Primary firmware slot (A).
    pub primary: FlashRegion,
    /// Secondary firmware slot (B) — OTA download target.
    pub secondary: FlashRegion,
    /// Persistent configuration region.
    pub config: FlashRegion,
    /// Audit log region.
    pub audit: FlashRegion,
}

impl FlashLayout {
    /// Build a layout for a given flash size and sector layout.
    ///
    /// `total_size` — total flash size in bytes.
    /// `sector_size` — erase sector size (typically 4 KiB or 64 KiB).
    /// `bootloader_size` — bootloader size in bytes.
    pub const fn new(total_size: u32, sector_size: u32, bootloader_size: u32) -> Self {
        let config_size = 16 * 1024;
        let audit_size = 8 * 1024;
        let slots = (total_size - bootloader_size - config_size - audit_size) / 2;

        Self {
            bootloader: FlashRegion {
                start: 0,
                end: bootloader_size - 1,
            },
            primary: FlashRegion {
                start: bootloader_size,
                end: bootloader_size + slots - 1,
            },
            secondary: FlashRegion {
                start: bootloader_size + slots,
                end: total_size - config_size - audit_size - 1,
            },
            config: FlashRegion {
                start: total_size - config_size - audit_size,
                end: total_size - audit_size - 1,
            },
            audit: FlashRegion {
                start: total_size - audit_size,
                end: total_size - 1,
            },
        }
    }

    /// Build a layout for QEMU virt (512 KiB flash, 4 KiB sectors).
    pub const fn qemu_virt() -> Self {
        // QEMU virt: simulated flash at 0x1000_0000, 512 KiB.
        let base = 0x1000_0000u32;
        let bootloader_size = 32 * 1024;
        let config_size = 16 * 1024;
        let audit_size = 8 * 1024;
        let slots = (512 * 1024 - bootloader_size - config_size - audit_size) / 2;

        Self {
            bootloader: FlashRegion { start: base, end: base + bootloader_size - 1 },
            primary: FlashRegion { start: base + bootloader_size, end: base + bootloader_size + slots - 1 },
            secondary: FlashRegion { start: base + bootloader_size + slots, end: base + 512 * 1024 - config_size - audit_size - 1 },
            config: FlashRegion { start: base + 512 * 1024 - config_size - audit_size, end: base + 512 * 1024 - audit_size - 1 },
            audit: FlashRegion { start: base + 512 * 1024 - audit_size, end: base + 512 * 1024 - 1 },
        }
    }

    /// Return the active (currently-running) slot based on boot flags.
    ///
    /// The bootloader writes a magic value to the config region at boot.
    /// This reads that magic to decide which slot was booted.
    pub fn active_slot(&self) -> FlashRegion {
        // In a real implementation, this would read the boot flag from
        // the config region. For now, always return primary.
        self.primary
    }

    /// Return the inactive slot (the OTA update target).
    pub fn inactive_slot(&self) -> FlashRegion {
        self.secondary
    }
}

/// Marker type for the [`SecurityToken<Flash>`] instance.
pub enum Flash {}

/// Flash capability token. Bundled with the region the holder
/// may write.
pub struct FlashToken {
    inner: SecurityToken<Flash>,
    region: FlashRegion,
}

impl FlashToken {
    /// **Kernel-only** constructor.
    ///
    /// # Safety
    /// Caller must hold the kernel `Cap<Flash>` and must verify
    /// that `region` does not overlap any currently executing
    /// code section.
    pub(crate) unsafe fn new(region: FlashRegion) -> Self {
        Self {
            inner: SecurityToken::<Flash>::new(),
            region,
        }
    }

    /// Region this token grants access to.
    pub fn region(&self) -> FlashRegion {
        self.region
    }

    /// Raw nonce (for logging / audit).
    pub fn nonce(&self) -> crate::token::CNonce {
        self.inner.nonce()
    }
}

// SAFETY: flash tokens are passed by-value between tasks via
// the capability table; the table itself is protected by the
// kernel spin-lock.
unsafe impl Send for FlashToken {}
unsafe impl Sync for FlashToken {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_contains_inclusive_endpoints() {
        let r = FlashRegion::new(0x0800_0000, 0x0800_FFFF).unwrap();
        assert!(r.contains(0x0800_0000, 0x0800_FFFF));
        assert!(r.contains(0x0800_1234, 0x0800_5678));
        assert!(!r.contains(0x0800_0000, 0x0801_0000));
        assert!(!r.contains(0x070F_FFFF, 0x0800_0000));
    }

    #[test]
    fn inverted_region_is_rejected() {
        assert!(FlashRegion::new(0x100, 0x80).is_none());
    }
}