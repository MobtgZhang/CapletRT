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
    /// Erase sector size in bytes.
    pub sector_size: u32,
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
            sector_size,
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
            bootloader: FlashRegion {
                start: base,
                end: base + bootloader_size - 1,
            },
            primary: FlashRegion {
                start: base + bootloader_size,
                end: base + bootloader_size + slots - 1,
            },
            secondary: FlashRegion {
                start: base + bootloader_size + slots,
                end: base + 512 * 1024 - config_size - audit_size - 1,
            },
            config: FlashRegion {
                start: base + 512 * 1024 - config_size - audit_size,
                end: base + 512 * 1024 - audit_size - 1,
            },
            audit: FlashRegion {
                start: base + 512 * 1024 - audit_size,
                end: base + 512 * 1024 - 1,
            },
            sector_size: 4096,
        }
    }

    /// Return the slot selected by an encoded boot flag value.
    ///
    /// The low 32 bits contain the boot magic and bits 32..48 contain the
    /// slot index (0 = primary, 1 = secondary). Unknown magics always select
    /// primary, which is the safe recovery image.
    pub const fn active_slot_from_flags(&self, flags: u64) -> FlashRegion {
        let magic = flags as u32;
        let slot = (flags >> 32) as u16;
        if (magic == BOOT_MAGIC_BOOTED || magic == BOOT_MAGIC_TRYING) && slot == 1 {
            self.secondary
        } else {
            self.primary
        }
    }

    /// Return the active (currently-running) slot based on boot flags.
    ///
    /// The bootloader writes a magic value to the config region at boot.
    /// This reads that magic to decide which slot was booted.
    pub fn active_slot(&self) -> FlashRegion {
        let magic_ptr = self.config.start as *const u32;
        let slot_ptr = (self.config.start + 4) as *const u16;
        // SAFETY: `config` is a board-supplied region. Callers that cannot
        // memory-map flash should use `active_slot_from_flags` with a value
        // obtained through their flash driver instead.
        let magic = unsafe { core::ptr::read_volatile(magic_ptr) };
        let slot = unsafe { core::ptr::read_volatile(slot_ptr) };
        self.active_slot_from_flags(encode_boot_flags(magic, slot))
    }

    /// Return the inactive slot (the OTA update target).
    pub fn inactive_slot(&self) -> FlashRegion {
        if self.active_slot() == self.primary {
            self.secondary
        } else {
            self.primary
        }
    }
}

/// No boot record / unknown magic.
pub const BOOT_MAGIC_NONE: u32 = 0;
/// The selected slot completed first boot successfully.
pub const BOOT_MAGIC_BOOTED: u32 = 0xB007_0001;
/// The selected slot is being tried and may still be rolled back.
pub const BOOT_MAGIC_TRYING: u32 = 0x5741_0002;

/// Encode a boot magic and slot index for [`FlashLayout::active_slot_from_flags`].
pub const fn encode_boot_flags(magic: u32, slot: u16) -> u64 {
    (magic as u64) | ((slot as u64) << 32)
}

/// Decode the value produced by [`encode_boot_flags`].
pub const fn decode_boot_flags(flags: u64) -> (u32, u16) {
    (flags as u32, (flags >> 32) as u16)
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
    pub unsafe fn new(region: FlashRegion) -> Self {
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

    #[test]
    fn boot_flags_select_the_correct_slot() {
        let layout = FlashLayout::new(1024 * 1024, 4096, 32 * 1024);
        assert_eq!(
            layout.active_slot_from_flags(encode_boot_flags(BOOT_MAGIC_BOOTED, 0)),
            layout.primary
        );
        assert_eq!(
            layout.active_slot_from_flags(encode_boot_flags(BOOT_MAGIC_TRYING, 1)),
            layout.secondary
        );
        assert_eq!(layout.active_slot_from_flags(0), layout.primary);
        assert_eq!(
            decode_boot_flags(encode_boot_flags(BOOT_MAGIC_BOOTED, 1)),
            (BOOT_MAGIC_BOOTED, 1)
        );
    }

    #[test]
    fn qemu_layout_is_sector_aligned() {
        let layout = FlashLayout::qemu_virt();
        for region in [
            layout.bootloader,
            layout.primary,
            layout.secondary,
            layout.config,
            layout.audit,
        ] {
            assert_eq!(region.start % layout.sector_size, 0);
            assert_eq!((region.end + 1) % layout.sector_size, 0);
        }
    }
}
