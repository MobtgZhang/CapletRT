//! Boot manager — A/B slot selection and boot flag handling.
//!
//! Implements the dual-bank OTA strategy:
//!   - Two firmware slots: `Primary` (A) and `Secondary` (B)
//!   - At boot time, `BootManager::select_slot()` reads boot flags from
//!     the Config flash region and picks the slot to run.
//!   - After a successful boot, `mark_booted()` writes the boot magic.
//!   - After a successful update, `mark_update_success()` clears the
//!     "trying" magic so the next boot goes directly to the new slot.

use caplet_security::flash::FlashLayout;

/// Boot slot — one of the two firmware partitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootSlot {
    /// Primary slot (A) — typically the first firmware.
    Primary,
    /// Secondary slot (B) — the OTA download target.
    Secondary,
}

impl BootSlot {
    /// Return the opposite slot.
    pub fn other(self) -> Self {
        match self {
            BootSlot::Primary => BootSlot::Secondary,
            BootSlot::Secondary => BootSlot::Primary,
        }
    }
}

/// Boot state — tracks where we are in the boot lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootState {
    /// Normal boot — pick the best available slot.
    Normal,
    /// Booting from the secondary slot after an update.
    TryingUpdate,
    /// The last update attempt failed; rolled back.
    RolledBack,
}

/// Magic values written to the Config region to track boot state.
///
/// Magic layout (8 bytes at Config.start):
///   [0..3]  : magic u32 (BOOTED = 0xB007_0001, TRYING = 0x5741_0002)
///   [4..5]  : slot u16 (0 = Primary, 1 = Secondary)
///   [6..7]  : version u16 (firmware semantic version)
///
/// Offset 8+: reserved for future use (boot count, failure reason, etc.).
const MAGIC_BOOTED: u32 = 0xB007_0001;
const MAGIC_TRYING: u32 = 0x5741_0002;

const FLAG_OFFSET_BOOT_MAGIC: u32 = 0;
const FLAG_OFFSET_SLOT: u32 = 4;
const FLAG_OFFSET_VERSION: u32 = 6;
const FLAG_SIZE: u32 = 8;

/// Boot manager — the central state machine for A/B slot management.
///
/// # Usage
/// ```rust
/// let bm = BootManager::new(layout);
/// let slot = bm.select_slot();
/// // ... boot from slot ...
/// bm.mark_booted(slot);
/// ```
pub struct BootManager<L: FlashLayoutApi = caplet_security::flash::FlashLayout> {
    layout: L,
}

impl<L: FlashLayoutApi> BootManager<L> {
    /// Create a new boot manager with the given flash layout.
    pub const fn new(layout: L) -> Self {
        Self { layout }
    }

    /// Select the slot to boot from.
    ///
    /// Priority:
    ///   1. If TRYING magic is set → boot the flagged slot (in-progress update).
    ///   2. If BOOTED magic is set → boot the flagged slot.
    ///   3. Otherwise → always boot Primary (first boot).
    pub fn select_slot(&self) -> BootSlot {
        let (magic, slot_raw) = self.read_flags();
        match magic {
            MAGIC_TRYING | MAGIC_BOOTED => {
                if slot_raw == 0 {
                    BootSlot::Primary
                } else {
                    BootSlot::Secondary
                }
            }
            _ => BootSlot::Primary,
        }
    }

    /// Return the boot state.
    pub fn boot_state(&self) -> BootState {
        let (magic, _) = self.read_flags();
        match magic {
            MAGIC_TRYING => BootState::TryingUpdate,
            MAGIC_BOOTED => BootState::Normal,
            _ => BootState::Normal,
        }
    }

    /// Mark that we have successfully booted from `slot`.
    ///
    /// This is called after the firmware in `slot` has initialized and
    /// is running correctly. It updates the boot magic to BOOTED.
    pub fn mark_booted(&self, slot: BootSlot) {
        let slot_raw = match slot {
            BootSlot::Primary => 0,
            BootSlot::Secondary => 1,
        };
        self.write_flags(MAGIC_BOOTED, slot_raw, 0);
    }

    /// Mark that an update is being attempted — set TRYING magic.
    ///
    /// Call this before jumping to the new firmware so that if the
    /// firmware fails to boot, the bootloader knows to roll back.
    pub fn mark_update_start(&self, slot: BootSlot) {
        let slot_raw = match slot {
            BootSlot::Primary => 0,
            BootSlot::Secondary => 1,
        };
        self.write_flags(MAGIC_TRYING, slot_raw, 0);
    }

    /// Mark that an update completed successfully — clear TRYING magic.
    ///
    /// Call this after the new firmware reports successful first boot.
    pub fn mark_update_success(&self) {
        // Clear the TRYING magic by writing 0.
        self.clear_flags();
    }

    /// Mark that an update failed and trigger rollback.
    ///
    /// The rollback is handled by `RollbackGuard` in the next phase.
    pub fn mark_update_failed(&self) {
        // Set the magic to BOOTED on the opposite slot.
        let (magic, _) = self.read_flags();
        let current = if magic == MAGIC_TRYING { self.select_slot() } else { BootSlot::Primary };
        let rollback_to = current.other();
        self.mark_booted(rollback_to);
    }

    // ── Internal ────────────────────────────────────────────────────────

    fn read_flags(&self) -> (u32, u16) {
        let config_base = self.layout.config_region().start;
        let magic = self.read_u32(config_base + FLAG_OFFSET_BOOT_MAGIC);
        let slot = self.read_u16(config_base + FLAG_OFFSET_SLOT);
        (magic, slot)
    }

    fn write_flags(&self, magic: u32, slot: u16, _version: u16) {
        let config_base = self.layout.config_region().start;
        self.write_u32(config_base + FLAG_OFFSET_BOOT_MAGIC, magic);
        self.write_u16(config_base + FLAG_OFFSET_SLOT, slot);
    }

    fn clear_flags(&self) {
        let config_base = self.layout.config_region().start;
        self.write_u32(config_base + FLAG_OFFSET_BOOT_MAGIC, 0);
    }

    fn read_u32(&self, addr: u32) -> u32 {
        let ptr = addr as *const u32;
        // Safety: addr must be within the Config flash region.
        unsafe { core::ptr::read_volatile(ptr) }
    }

    fn read_u16(&self, addr: u32) -> u16 {
        let ptr = addr as *const u16;
        unsafe { core::ptr::read_volatile(ptr) }
    }

    fn write_u32(&self, addr: u32, val: u32) {
        let ptr = addr as *mut u32;
        // Safety: caller must hold FlashToken for the Config region.
        unsafe { core::ptr::write_volatile(ptr, val) };
    }

    fn write_u16(&self, addr: u32, val: u16) {
        let ptr = addr as *mut u16;
        unsafe { core::ptr::write_volatile(ptr, val) };
    }
}

/// Trait abstracting flash read/write for the boot manager.
///
/// Implementors provide the actual flash I/O. The default implementation
/// uses volatile MMIO reads (suitable for MCU flash controllers).
pub trait FlashLayoutApi {
    /// Return the config flash region.
    fn config_region(&self) -> caplet_security::flash::FlashRegion;
}

/// Implement FlashLayoutApi for the standard FlashLayout.
impl FlashLayoutApi for caplet_security::flash::FlashLayout {
    fn config_region(&self) -> caplet_security::flash::FlashRegion {
        self.config
    }
}

impl FlashLayoutApi for caplet_security::flash::FlashRegion {
    fn config_region(&self) -> caplet_security::flash::FlashRegion {
        *self
    }
}
