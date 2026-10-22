//! Power-fail-safe A/B boot selection.
//!
//! Boot flags are stored as two alternating records in separate erase
//! sectors of the config partition.  A record is only considered valid after
//! its CRC has been checked.  Updating flags erases and programs the replica
//! that does *not* hold the newest record, so a power loss at any point leaves
//! at least one valid record behind.

use caplet_security::flash::{
    decode_boot_flags, encode_boot_flags, FlashLayout, FlashRegion, BOOT_MAGIC_BOOTED,
    BOOT_MAGIC_NONE, BOOT_MAGIC_TRYING,
};

use crate::flash::{FlashIoError, FlashStorage};

/// Boot slot — one of the two firmware partitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootSlot {
    /// Primary slot (A).
    Primary,
    /// Secondary slot (B), normally the OTA target.
    Secondary,
}

impl BootSlot {
    /// Return the opposite slot.
    pub const fn other(self) -> Self {
        match self {
            BootSlot::Primary => BootSlot::Secondary,
            BootSlot::Secondary => BootSlot::Primary,
        }
    }

    /// Encode as the on-flash slot index.
    pub const fn index(self) -> u16 {
        match self {
            BootSlot::Primary => 0,
            BootSlot::Secondary => 1,
        }
    }

    /// Decode a valid on-flash slot index.
    pub const fn from_index(index: u16) -> Option<Self> {
        match index {
            0 => Some(BootSlot::Primary),
            1 => Some(BootSlot::Secondary),
            _ => None,
        }
    }

    /// Region for this slot in `layout`.
    pub const fn region(self, layout: &FlashLayout) -> FlashRegion {
        match self {
            BootSlot::Primary => layout.primary,
            BootSlot::Secondary => layout.secondary,
        }
    }
}

/// Boot lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootState {
    /// The selected slot has successfully booted before.
    Normal,
    /// The selected slot is a trial boot and can still be rolled back.
    TryingUpdate,
    /// The last trial failed and the bootloader fell back.
    RolledBack,
}

/// Validated boot configuration record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootRecord {
    /// Monotonically increasing sequence number.
    pub sequence: u32,
    /// Selected slot.
    pub slot: BootSlot,
    /// Firmware version associated with the slot.
    pub version: u32,
    /// Lifecycle state.
    pub state: BootState,
    /// Physical replica index (0 or 1), not part of the CRC.
    replica: u8,
}

const RECORD_MAGIC: u32 = 0x4246_5231; // 'BFR1'
const RECORD_SIZE: usize = 32;
const RECORD_CRC_LEN: usize = 28;

impl BootRecord {
    fn encode(&self) -> [u8; RECORD_SIZE] {
        let flags = match self.state {
            BootState::TryingUpdate => encode_boot_flags(BOOT_MAGIC_TRYING, self.slot.index()),
            BootState::Normal | BootState::RolledBack => {
                encode_boot_flags(BOOT_MAGIC_BOOTED, self.slot.index())
            }
        };
        let status = match self.state {
            BootState::Normal => 0u32,
            BootState::TryingUpdate => 1,
            BootState::RolledBack => 2,
        };
        let mut out = [0u8; RECORD_SIZE];
        out[0..4].copy_from_slice(&RECORD_MAGIC.to_le_bytes());
        out[4..8].copy_from_slice(&self.sequence.to_le_bytes());
        out[8..16].copy_from_slice(&flags.to_le_bytes());
        out[16..20].copy_from_slice(&self.version.to_le_bytes());
        out[20..24].copy_from_slice(&status.to_le_bytes());
        let crc = crc32(&out[..RECORD_CRC_LEN]);
        out[28..32].copy_from_slice(&crc.to_le_bytes());
        out
    }

    fn decode(bytes: &[u8; RECORD_SIZE], replica: u8) -> Option<Self> {
        if u32::from_le_bytes(bytes[0..4].try_into().ok()?) != RECORD_MAGIC {
            return None;
        }
        let expected = u32::from_le_bytes(bytes[28..32].try_into().ok()?);
        if crc32(&bytes[..RECORD_CRC_LEN]) != expected {
            return None;
        }
        let flags = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
        let (magic, slot_index) = decode_boot_flags(flags);
        if magic != BOOT_MAGIC_BOOTED && magic != BOOT_MAGIC_TRYING {
            return None;
        }
        let slot = BootSlot::from_index(slot_index)?;
        let status = u32::from_le_bytes(bytes[20..24].try_into().ok()?);
        let state = match status {
            0 => BootState::Normal,
            1 => BootState::TryingUpdate,
            2 => BootState::RolledBack,
            _ => return None,
        };
        Some(Self {
            sequence: u32::from_le_bytes(bytes[4..8].try_into().ok()?),
            slot,
            version: u32::from_le_bytes(bytes[16..20].try_into().ok()?),
            state,
            replica,
        })
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Sequence comparison that remains correct across wrapping.
const fn sequence_is_newer(a: u32, b: u32) -> bool {
    a.wrapping_sub(b) < 0x8000_0000
}

/// Manages A/B boot flags over a flash driver.
pub struct BootManager<F: FlashStorage> {
    layout: FlashLayout,
    flash: F,
}

impl<F: FlashStorage> BootManager<F> {
    /// Create a boot manager.
    pub const fn new(layout: FlashLayout, flash: F) -> Self {
        Self { layout, flash }
    }

    /// Access the flash driver (for diagnostics and tests).
    pub fn flash_mut(&mut self) -> &mut F {
        &mut self.flash
    }

    /// Return the newest valid record, if one exists.
    pub fn latest_record(&mut self) -> Result<Option<BootRecord>, FlashIoError> {
        let mut best = None;
        for replica in 0..2u8 {
            let addr = self.replica_addr(replica)?;
            let mut bytes = [0u8; RECORD_SIZE];
            self.flash.read(addr, &mut bytes)?;
            if let Some(record) = BootRecord::decode(&bytes, replica) {
                match best {
                    None => best = Some(record),
                    Some(current) if sequence_is_newer(record.sequence, current.sequence) => {
                        best = Some(record)
                    }
                    _ => {}
                }
            }
        }
        Ok(best)
    }

    /// Select the slot to boot from.
    pub fn select_slot(&mut self) -> Result<BootSlot, FlashIoError> {
        Ok(self.latest_record()?.map_or(BootSlot::Primary, |r| r.slot))
    }

    /// Return the current lifecycle state.
    pub fn boot_state(&mut self) -> Result<BootState, FlashIoError> {
        Ok(self
            .latest_record()?
            .map_or(BootState::Normal, |record| record.state))
    }

    /// Mark a slot as successfully booted.
    pub fn mark_booted(&mut self, slot: BootSlot, version: u32) -> Result<(), FlashIoError> {
        self.write_record(slot, version, BootState::Normal)
    }

    /// Mark a slot as a trial boot.
    pub fn mark_update_start(&mut self, slot: BootSlot, version: u32) -> Result<(), FlashIoError> {
        self.write_record(slot, version, BootState::TryingUpdate)
    }

    /// Confirm a previously started update.
    pub fn mark_update_success(
        &mut self,
        slot: BootSlot,
        version: u32,
    ) -> Result<(), FlashIoError> {
        self.mark_booted(slot, version)
    }

    /// Roll back from the current trial slot to the opposite slot.
    pub fn mark_update_failed(&mut self) -> Result<BootSlot, FlashIoError> {
        let rollback_to = self
            .latest_record()?
            .map_or(BootSlot::Primary, |record| record.slot.other());
        self.write_record(rollback_to, 0, BootState::RolledBack)?;
        Ok(rollback_to)
    }

    fn write_record(
        &mut self,
        slot: BootSlot,
        version: u32,
        state: BootState,
    ) -> Result<(), FlashIoError> {
        let latest = self.latest_record()?;
        let target_replica = latest.map_or(0u8, |record| 1 - record.replica);
        let sequence = latest.map_or(1, |record| record.sequence.wrapping_add(1));
        let record = BootRecord {
            sequence,
            slot,
            version,
            state,
            replica: target_replica,
        };
        let encoded = record.encode();
        let addr = self.replica_addr(target_replica)?;

        self.flash.unlock()?;
        let result = (|| {
            self.flash.erase(addr, self.layout.sector_size)?;
            self.flash.program(addr, &encoded)?;
            let mut readback = [0u8; RECORD_SIZE];
            self.flash.read(addr, &mut readback)?;
            if readback != encoded {
                return Err(FlashIoError::Io);
            }
            Ok(())
        })();
        let lock_result = self.flash.lock();
        result.and(lock_result)
    }

    fn replica_addr(&self, replica: u8) -> Result<u32, FlashIoError> {
        let offset = (replica as u32)
            .checked_mul(self.layout.sector_size)
            .ok_or(FlashIoError::OutOfRange)?;
        let addr = self
            .layout
            .config
            .start
            .checked_add(offset)
            .ok_or(FlashIoError::OutOfRange)?;
        let end = addr
            .checked_add(self.layout.sector_size)
            .and_then(|v| v.checked_sub(1))
            .ok_or(FlashIoError::OutOfRange)?;
        if !self.layout.config.contains(addr, end) {
            return Err(FlashIoError::OutOfRange);
        }
        Ok(addr)
    }
}

/// Decode the boot flags embedded in a record for callers that only need the
/// canonical boot-selection value.
pub fn record_boot_flags(record: BootRecord) -> u64 {
    let magic = match record.state {
        BootState::TryingUpdate => BOOT_MAGIC_TRYING,
        BootState::Normal | BootState::RolledBack => BOOT_MAGIC_BOOTED,
    };
    encode_boot_flags(magic, record.slot.index())
}

/// Empty/no-record value for callers initialising a fresh config region.
pub const fn no_boot_flags() -> u64 {
    encode_boot_flags(BOOT_MAGIC_NONE, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flash::RamFlash;

    const FLASH_SIZE: usize = 256 * 1024;

    fn fixture() -> (BootManager<RamFlash<FLASH_SIZE>>, FlashLayout) {
        let layout = FlashLayout::new(FLASH_SIZE as u32, 4096, 32 * 1024);
        let flash = RamFlash::new(0, 4096);
        (BootManager::new(layout, flash), layout)
    }

    #[test]
    fn first_boot_selects_primary() {
        let (mut manager, _) = fixture();
        assert_eq!(manager.select_slot().unwrap(), BootSlot::Primary);
        assert_eq!(manager.boot_state().unwrap(), BootState::Normal);
        assert!(manager.latest_record().unwrap().is_none());
    }

    #[test]
    fn trial_update_selects_secondary() {
        let (mut manager, _) = fixture();
        manager
            .mark_update_start(BootSlot::Secondary, 0x0001_0200)
            .unwrap();
        assert_eq!(manager.select_slot().unwrap(), BootSlot::Secondary);
        assert_eq!(manager.boot_state().unwrap(), BootState::TryingUpdate);
        assert_eq!(
            manager.latest_record().unwrap().unwrap().version,
            0x0001_0200
        );
    }

    #[test]
    fn success_confirms_secondary() {
        let (mut manager, _) = fixture();
        manager.mark_update_start(BootSlot::Secondary, 2).unwrap();
        manager.mark_update_success(BootSlot::Secondary, 2).unwrap();
        assert_eq!(manager.select_slot().unwrap(), BootSlot::Secondary);
        assert_eq!(manager.boot_state().unwrap(), BootState::Normal);
    }

    #[test]
    fn failure_rolls_back_to_primary() {
        let (mut manager, _) = fixture();
        manager.mark_update_start(BootSlot::Secondary, 2).unwrap();
        assert_eq!(manager.mark_update_failed().unwrap(), BootSlot::Primary);
        assert_eq!(manager.select_slot().unwrap(), BootSlot::Primary);
        assert_eq!(manager.boot_state().unwrap(), BootState::RolledBack);
    }

    #[test]
    fn records_alternate_between_erase_sectors() {
        let (mut manager, _) = fixture();
        manager.mark_booted(BootSlot::Primary, 1).unwrap();
        let first = manager.latest_record().unwrap().unwrap();
        manager.mark_booted(BootSlot::Primary, 2).unwrap();
        let second = manager.latest_record().unwrap().unwrap();
        assert_ne!(first.replica, second.replica);
        assert_eq!(second.sequence, first.sequence + 1);
        assert_eq!(second.version, 2);
    }

    #[test]
    fn latest_valid_replica_survives_corruption_of_the_other() {
        let (mut manager, layout) = fixture();
        manager.mark_booted(BootSlot::Primary, 1).unwrap();
        manager.mark_update_start(BootSlot::Secondary, 3).unwrap();
        // Corrupt the older replica. The newest trial record must still win.
        let addr = layout.config.start;
        manager.flash_mut().as_mut_slice()[addr as usize + 4] ^= 0xFF;
        assert_eq!(manager.select_slot().unwrap(), BootSlot::Secondary);
    }

    #[test]
    fn power_loss_while_writing_keeps_previous_record() {
        let (mut manager, _) = fixture();
        manager.mark_booted(BootSlot::Primary, 1).unwrap();
        manager.flash_mut().inject_power_cut_after(0);
        assert_eq!(
            manager.mark_update_start(BootSlot::Secondary, 2),
            Err(FlashIoError::PowerCut)
        );
        assert_eq!(manager.select_slot().unwrap(), BootSlot::Primary);
    }

    #[test]
    fn all_erased_or_garbage_flags_default_to_primary() {
        let (mut manager, layout) = fixture();
        manager.flash_mut().as_mut_slice().fill(0x5A);
        assert!(manager.latest_record().unwrap().is_none());
        assert_eq!(manager.select_slot().unwrap(), BootSlot::Primary);
        assert_eq!(
            layout.active_slot_from_flags(no_boot_flags()),
            layout.primary
        );
    }

    #[test]
    fn record_crc_rejects_mutation() {
        let record = BootRecord {
            sequence: 1,
            slot: BootSlot::Secondary,
            version: 7,
            state: BootState::TryingUpdate,
            replica: 1,
        };
        let mut bytes = record.encode();
        assert!(BootRecord::decode(&bytes, 1).is_some());
        bytes[16] ^= 1;
        assert!(BootRecord::decode(&bytes, 1).is_none());
    }

    #[test]
    fn boot_flags_round_trip_between_ota_and_security_layouts() {
        let record = BootRecord {
            sequence: 1,
            slot: BootSlot::Secondary,
            version: 1,
            state: BootState::TryingUpdate,
            replica: 0,
        };
        let (_, layout) = fixture();
        assert_eq!(
            layout.active_slot_from_flags(record_boot_flags(record)),
            layout.secondary
        );
    }
}
