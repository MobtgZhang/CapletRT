//! Firmware download and A/B installation.
//!
//! The updater writes only the inactive slot, verifies the written bytes from
//! flash, and only then marks the slot as a trial boot.  A failed download or
//! authentication error therefore cannot affect the currently bootable slot.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use caplet_security::flash::{FlashLayout, FlashRegion};

use crate::boot_manager::{BootManager, BootSlot};
use crate::flash::{FlashIoError, FlashStorage};
use crate::verifier::{
    FirmwareHeader, TrustStore, Verifier, VerifyError, HEADER_SIZE, MIN_SLOT_SIZE, SIGNATURE_SIZE,
};

/// A source of firmware bytes.  Sources may be network transports, a flash
/// staging area, or a test fixture.
pub trait FirmwareSource {
    /// Number of image bytes available.
    fn len(&self) -> usize;

    /// Whether the source contains no image bytes.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read exactly `dst.len()` bytes beginning at `offset`.
    fn read(&mut self, offset: usize, dst: &mut [u8]) -> Result<(), DownloadError>;
}

/// In-memory firmware source.
pub struct SliceSource<'a> {
    bytes: &'a [u8],
}

impl<'a> SliceSource<'a> {
    /// Wrap a byte slice.
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl FirmwareSource for SliceSource<'_> {
    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn read(&mut self, offset: usize, dst: &mut [u8]) -> Result<(), DownloadError> {
        let end = offset
            .checked_add(dst.len())
            .ok_or(DownloadError::SourceOutOfRange)?;
        let source = self
            .bytes
            .get(offset..end)
            .ok_or(DownloadError::SourceOutOfRange)?;
        dst.copy_from_slice(source);
        Ok(())
    }
}

/// Errors produced by an OTA installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadError {
    /// Image is empty.
    EmptyImage,
    /// Image plus metadata does not fit in one slot.
    ImageTooLarge,
    /// Source read failed.
    SourceOutOfRange,
    /// Signature or hash verification failed.
    Verification(VerifyError),
    /// Flash operation failed.
    Flash(FlashIoError),
}

impl From<FlashIoError> for DownloadError {
    fn from(value: FlashIoError) -> Self {
        DownloadError::Flash(value)
    }
}

impl From<VerifyError> for DownloadError {
    fn from(value: VerifyError) -> Self {
        DownloadError::Verification(value)
    }
}

/// Result of a successful installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateResult {
    /// Slot that now contains the verified image.
    pub slot: BootSlot,
    /// Verified firmware header.
    pub header: FirmwareHeader,
}

/// Download/install engine.
pub struct Updater<F: FlashStorage> {
    layout: FlashLayout,
    flash: F,
    keys: TrustStore,
}

impl<F: FlashStorage> Updater<F> {
    /// Construct an updater.
    pub const fn new(layout: FlashLayout, flash: F, keys: TrustStore) -> Self {
        Self {
            layout,
            flash,
            keys,
        }
    }

    /// Install a signed firmware image into the inactive slot.
    ///
    /// Authentication is performed before any flash erase, then repeated from
    /// the actual flash contents before the trial flag is published.
    pub fn install_signed(
        &mut self,
        image: &[u8],
        version: u32,
        public_key_id: u32,
        signature: &[u8; SIGNATURE_SIZE],
    ) -> Result<UpdateResult, DownloadError> {
        if image.is_empty() {
            return Err(DownloadError::EmptyImage);
        }
        let target = {
            let mut manager = BootManager::new(self.layout, &mut self.flash);
            manager.select_slot()?.other()
        };
        let region = target.region(&self.layout);
        let capacity = region_capacity(region);
        if image.len() > capacity.saturating_sub(MIN_SLOT_SIZE) {
            return Err(DownloadError::ImageTooLarge);
        }

        let header = FirmwareHeader::new(version, image, public_key_id);
        Verifier::new().verify_parts(&header, image, signature, &self.keys)?;

        self.program_slot(region, &header, image, signature)?;
        let verified = Verifier::new().verify_flash(&mut self.flash, region, &self.keys)?;
        if verified != header {
            return Err(DownloadError::Verification(VerifyError::HashMismatch));
        }

        let mut manager = BootManager::new(self.layout, &mut self.flash);
        manager.mark_update_start(target, version)?;
        Ok(UpdateResult {
            slot: target,
            header: verified,
        })
    }

    /// Read a source in bounded chunks and install it.
    pub fn install_source(
        &mut self,
        source: &mut impl FirmwareSource,
        version: u32,
        public_key_id: u32,
        signature: &[u8; SIGNATURE_SIZE],
    ) -> Result<UpdateResult, DownloadError> {
        if source.len() == 0 {
            return Err(DownloadError::EmptyImage);
        }
        let mut image = vec![0u8; source.len()];
        const CHUNK: usize = 256;
        let mut offset = 0usize;
        while offset < image.len() {
            let end = (offset + CHUNK).min(image.len());
            source.read(offset, &mut image[offset..end])?;
            offset = end;
        }
        self.install_signed(&image, version, public_key_id, signature)
    }

    fn program_slot(
        &mut self,
        region: FlashRegion,
        header: &FirmwareHeader,
        image: &[u8],
        signature: &[u8; SIGNATURE_SIZE],
    ) -> Result<(), DownloadError> {
        let capacity = region_capacity(region);
        let signature_addr = region.end + 1 - SIGNATURE_SIZE as u32;
        self.flash.unlock()?;
        let result = (|| {
            self.flash.erase(region.start, capacity as u32)?;
            self.flash.program(region.start, &header.to_bytes())?;
            self.flash
                .program(region.start + HEADER_SIZE as u32, image)?;
            self.flash.program(signature_addr, signature)?;
            Ok(())
        })();
        let lock_result = self.flash.lock();
        match (result, lock_result) {
            (Err(error), _) => Err(error),
            (_, Err(error)) => Err(error.into()),
            _ => Ok(()),
        }
    }
}

fn region_capacity(region: FlashRegion) -> usize {
    region.end.saturating_sub(region.start).saturating_add(1) as usize
}

/// Verify an entire update without modifying flash.
pub fn preflight(
    source: &mut impl FirmwareSource,
    version: u32,
    public_key_id: u32,
    signature: &[u8; SIGNATURE_SIZE],
    keys: &TrustStore,
) -> Result<FirmwareHeader, DownloadError> {
    let mut image = vec![0u8; source.len()];
    source.read(0, &mut image)?;
    let header = FirmwareHeader::new(version, &image, public_key_id);
    Ok(Verifier::new().verify_parts(&header, &image, signature, keys)?)
}

/// Collect a source into an owned vector (useful for transports that deliver
/// multiple frames).
pub fn collect_source(source: &mut impl FirmwareSource) -> Result<Vec<u8>, DownloadError> {
    let mut image = vec![0u8; source.len()];
    source.read(0, &mut image)?;
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot_manager::{BootManager, BootState};
    use crate::flash::RamFlash;
    use ed25519_dalek::Signer;

    const FLASH_SIZE: usize = 256 * 1024;
    const KEY_ID: u32 = 11;

    fn layout() -> FlashLayout {
        FlashLayout::new(FLASH_SIZE as u32, 4096, 32 * 1024)
    }

    fn trust() -> (TrustStore, ed25519_dalek::SigningKey) {
        let key = ed25519_dalek::SigningKey::from_bytes(&[5u8; 32]);
        let mut store = TrustStore::new();
        assert!(store.add(KEY_ID, key.verifying_key().to_bytes()));
        (store, key)
    }

    fn sign(image: &[u8], version: u32, key: &ed25519_dalek::SigningKey) -> [u8; SIGNATURE_SIZE] {
        let header = FirmwareHeader::new(version, image, KEY_ID);
        let mut message = Vec::new();
        message.extend_from_slice(&header.to_bytes());
        message.extend_from_slice(image);
        key.sign(&message).to_bytes()
    }

    #[test]
    fn installs_signed_image_into_inactive_slot_and_marks_trial() {
        let (store, key) = trust();
        let layout = layout();
        let mut flash = RamFlash::<FLASH_SIZE>::new(0, 4096);
        let image = b"new firmware";
        let sig = sign(image, 2, &key);
        let mut updater = Updater::new(layout, &mut flash, store);
        let result = updater
            .install_signed(image, 2, KEY_ID, &sig)
            .expect("install succeeds");
        assert_eq!(result.slot, BootSlot::Secondary);
        let mut manager = BootManager::new(layout, &mut flash);
        assert_eq!(manager.select_slot().unwrap(), BootSlot::Secondary);
        assert_eq!(manager.boot_state().unwrap(), BootState::TryingUpdate);
    }

    #[test]
    fn invalid_signature_does_not_modify_flash() {
        let (store, key) = trust();
        let layout = layout();
        let mut flash = RamFlash::<FLASH_SIZE>::new(0, 4096);
        let image = b"new firmware";
        let mut sig = sign(image, 2, &key);
        sig[0] ^= 1;
        let before = flash.as_slice().to_vec();
        let mut updater = Updater::new(layout, &mut flash, store);
        assert!(matches!(
            updater.install_signed(image, 2, KEY_ID, &sig),
            Err(DownloadError::Verification(VerifyError::BadSignature))
        ));
        assert_eq!(flash.as_slice(), before);
    }

    #[test]
    fn rejects_empty_and_oversized_images() {
        let (store, key) = trust();
        let layout = layout();
        let mut flash = RamFlash::<FLASH_SIZE>::new(0, 4096);
        let empty_sig = sign(b"", 1, &key);
        let mut updater = Updater::new(layout, &mut flash, store);
        assert_eq!(
            updater.install_signed(b"", 1, KEY_ID, &empty_sig),
            Err(DownloadError::EmptyImage)
        );
        let huge = vec![0u8; (layout.secondary.end - layout.secondary.start) as usize];
        let sig = sign(&huge, 1, &key);
        assert_eq!(
            updater.install_signed(&huge, 1, KEY_ID, &sig),
            Err(DownloadError::ImageTooLarge)
        );
    }

    #[test]
    fn chunked_source_installs_same_bytes() {
        let (store, key) = trust();
        let layout = layout();
        let mut image = vec![0x41u8; 600];
        image[..8].copy_from_slice(b"chunked!");
        assert!(image.len() > 256);
        let sig = sign(&image, 7, &key);
        let mut flash = RamFlash::<FLASH_SIZE>::new(0, 4096);
        let mut updater = Updater::new(layout, &mut flash, store);
        let mut source = SliceSource::new(&image);
        let result = updater
            .install_source(&mut source, 7, KEY_ID, &sig)
            .unwrap();
        assert_eq!(result.header.image_len, image.len() as u32);
        let region = layout.secondary;
        let mut readback = vec![0u8; image.len()];
        flash
            .read(region.start + HEADER_SIZE as u32, &mut readback)
            .unwrap();
        assert_eq!(readback, image);
    }

    #[test]
    fn power_loss_during_install_does_not_publish_trial() {
        let (store, key) = trust();
        let layout = layout();
        let image = b"firmware";
        let sig = sign(image, 2, &key);
        let mut flash = RamFlash::<FLASH_SIZE>::new(0, 4096);
        flash.inject_power_cut_after(0);
        let mut updater = Updater::new(layout, &mut flash, store);
        assert!(matches!(
            updater.install_signed(image, 2, KEY_ID, &sig),
            Err(DownloadError::Flash(FlashIoError::PowerCut))
        ));
        assert_eq!(
            BootManager::new(layout, &mut flash).select_slot().unwrap(),
            BootSlot::Primary
        );
    }

    #[test]
    fn preflight_accepts_valid_signature() {
        let (store, key) = trust();
        let image = b"preflight";
        let sig = sign(image, 1, &key);
        let mut source = SliceSource::new(image);
        let header = preflight(&mut source, 1, KEY_ID, &sig, &store).unwrap();
        assert_eq!(header.image_len, image.len() as u32);
    }
}
