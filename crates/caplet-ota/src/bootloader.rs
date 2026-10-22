//! Secure-boot decision logic.
//!
//! The first-stage bootloader reads the A/B boot record, verifies the selected
//! firmware's SHA-256 and Ed25519 signature before permitting control transfer,
//! and rolls back to the opposite slot when a trial image is invalid.

use caplet_security::flash::{FlashLayout, FlashRegion};

use crate::boot_manager::{BootManager, BootSlot, BootState};
use crate::flash::{FlashIoError, FlashStorage};
use crate::verifier::{FirmwareHeader, TrustStore, Verifier, VerifyError};

/// A verified firmware slot ready to execute.
#[derive(Debug, Clone, Copy)]
pub struct BootSelection {
    /// Slot that must be executed.
    pub slot: BootSlot,
    /// Physical flash region containing the image.
    pub region: FlashRegion,
    /// Verified firmware header.
    pub header: FirmwareHeader,
    /// True when this is the first trial boot of an updated image.
    pub trial: bool,
    /// True when the bootloader had to fall back to the other slot.
    pub rolled_back: bool,
}

/// Reason the bootloader refused to hand control to firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootFailure {
    /// Flash access failed while reading boot flags.
    Flash(FlashIoError),
    /// Both slots failed authentication.
    NoBootableSlot {
        /// Verification result for the slot that was preferred first.
        selected: VerifyError,
        /// Verification result for the fallback slot.
        fallback: VerifyError,
    },
}

impl From<FlashIoError> for BootFailure {
    fn from(value: FlashIoError) -> Self {
        BootFailure::Flash(value)
    }
}

/// Result of a secure-boot attempt.
#[derive(Debug, Clone, Copy)]
pub enum BootOutcome {
    /// Authenticated slot; the caller may jump to `header.image`.
    Boot(BootSelection),
    /// Do not execute either slot.
    Halt(BootFailure),
}

/// Secure bootloader over an abstract NOR flash device.
pub struct Bootloader<F: FlashStorage> {
    layout: FlashLayout,
    flash: F,
    keys: TrustStore,
}

impl<F: FlashStorage> Bootloader<F> {
    /// Construct a bootloader with a fixed trust store.
    pub const fn new(layout: FlashLayout, flash: F, keys: TrustStore) -> Self {
        Self {
            layout,
            flash,
            keys,
        }
    }

    /// Select and authenticate a firmware slot.
    pub fn boot(&mut self) -> BootOutcome {
        let selection = {
            let mut manager = BootManager::new(self.layout, &mut self.flash);
            match manager.latest_record() {
                Ok(record) => record.map_or((BootSlot::Primary, BootState::Normal), |r| {
                    (r.slot, r.state)
                }),
                Err(error) => return BootOutcome::Halt(BootFailure::Flash(error)),
            }
        };
        let (selected_slot, state) = selection;
        let selected_region = selected_slot.region(&self.layout);

        match Verifier::new().verify_flash(&mut self.flash, selected_region, &self.keys) {
            Ok(header) => BootOutcome::Boot(BootSelection {
                slot: selected_slot,
                region: selected_region,
                header,
                trial: state == BootState::TryingUpdate,
                rolled_back: false,
            }),
            Err(selected_error) => {
                let fallback_slot = selected_slot.other();
                let fallback_region = fallback_slot.region(&self.layout);
                match Verifier::new().verify_flash(&mut self.flash, fallback_region, &self.keys) {
                    Ok(header) => {
                        let mut manager = BootManager::new(self.layout, &mut self.flash);
                        let _ = manager.mark_booted(fallback_slot, header.version);
                        BootOutcome::Boot(BootSelection {
                            slot: fallback_slot,
                            region: fallback_region,
                            header,
                            trial: false,
                            rolled_back: true,
                        })
                    }
                    Err(fallback_error) => BootOutcome::Halt(BootFailure::NoBootableSlot {
                        selected: selected_error,
                        fallback: fallback_error,
                    }),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verifier::{FirmwareHeader, HEADER_SIZE, SIGNATURE_SIZE};
    use ed25519_dalek::Signer;

    const FLASH_SIZE: usize = 256 * 1024;
    const KEY_ID: u32 = 9;

    fn layout() -> FlashLayout {
        FlashLayout::new(FLASH_SIZE as u32, 4096, 32 * 1024)
    }

    fn keys() -> (TrustStore, ed25519_dalek::SigningKey) {
        let key = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
        let mut store = TrustStore::new();
        assert!(store.add(KEY_ID, key.verifying_key().to_bytes()));
        (store, key)
    }

    fn write_firmware<F: FlashStorage>(
        flash: &mut F,
        region: FlashRegion,
        image: &[u8],
        version: u32,
        key: &ed25519_dalek::SigningKey,
    ) {
        let capacity = (region.end - region.start + 1) as usize;
        let header = FirmwareHeader::new(version, image, KEY_ID);
        let mut message = Vec::new();
        message.extend_from_slice(&header.to_bytes());
        message.extend_from_slice(image);
        let signature = key.sign(&message).to_bytes();

        flash.unlock().unwrap();
        flash.erase(region.start, capacity as u32).unwrap();
        flash.program(region.start, &header.to_bytes()).unwrap();
        flash
            .program(region.start + HEADER_SIZE as u32, image)
            .unwrap();
        flash
            .program(region.end + 1 - SIGNATURE_SIZE as u32, &signature)
            .unwrap();
        flash.lock().unwrap();
    }

    #[test]
    fn boots_valid_primary() {
        let (store, key) = keys();
        let layout = layout();
        let mut flash = crate::flash::RamFlash::<FLASH_SIZE>::new(0, 4096);
        write_firmware(&mut flash, layout.primary, b"primary", 1, &key);
        let mut loader = Bootloader::new(layout, flash, store);
        match loader.boot() {
            BootOutcome::Boot(selection) => {
                assert_eq!(selection.slot, BootSlot::Primary);
                assert!(!selection.trial);
                assert!(!selection.rolled_back);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn trial_secondary_is_reported_as_trial() {
        let (store, key) = keys();
        let layout = layout();
        let mut flash = crate::flash::RamFlash::<FLASH_SIZE>::new(0, 4096);
        write_firmware(&mut flash, layout.primary, b"primary", 1, &key);
        write_firmware(&mut flash, layout.secondary, b"secondary", 2, &key);
        BootManager::new(layout, &mut flash)
            .mark_update_start(BootSlot::Secondary, 2)
            .unwrap();
        let mut loader = Bootloader::new(layout, flash, store);
        match loader.boot() {
            BootOutcome::Boot(selection) => {
                assert_eq!(selection.slot, BootSlot::Secondary);
                assert!(selection.trial);
                assert!(!selection.rolled_back);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn tampered_trial_rolls_back_to_primary() {
        let (store, key) = keys();
        let layout = layout();
        let mut flash = crate::flash::RamFlash::<FLASH_SIZE>::new(0, 4096);
        write_firmware(&mut flash, layout.primary, b"primary", 1, &key);
        write_firmware(&mut flash, layout.secondary, b"secondary", 2, &key);
        BootManager::new(layout, &mut flash)
            .mark_update_start(BootSlot::Secondary, 2)
            .unwrap();
        flash.unlock().unwrap();
        flash
            .program(layout.secondary.start + HEADER_SIZE as u32, &[0])
            .unwrap();
        flash.lock().unwrap();
        let mut loader = Bootloader::new(layout, flash, store);
        match loader.boot() {
            BootOutcome::Boot(selection) => {
                assert_eq!(selection.slot, BootSlot::Primary);
                assert!(selection.rolled_back);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
        assert_eq!(
            BootManager::new(layout, loader.flash).boot_state().unwrap(),
            BootState::Normal
        );
    }

    #[test]
    fn both_invalid_halts() {
        let (store, _) = keys();
        let layout = layout();
        let flash = crate::flash::RamFlash::<FLASH_SIZE>::new(0, 4096);
        let mut loader = Bootloader::new(layout, flash, store);
        match loader.boot() {
            BootOutcome::Halt(BootFailure::NoBootableSlot { .. }) => {}
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn invalid_primary_falls_back_to_valid_secondary() {
        let (store, key) = keys();
        let layout = layout();
        let mut flash = crate::flash::RamFlash::<FLASH_SIZE>::new(0, 4096);
        write_firmware(&mut flash, layout.secondary, b"secondary", 2, &key);
        let mut loader = Bootloader::new(layout, flash, store);
        match loader.boot() {
            BootOutcome::Boot(selection) => {
                assert_eq!(selection.slot, BootSlot::Secondary);
                assert!(selection.rolled_back);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }
}
