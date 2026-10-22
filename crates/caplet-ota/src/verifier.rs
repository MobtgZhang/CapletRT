//! Firmware integrity and signature verification.
//!
//! A slot has the following fixed layout:
//!
//! ```text
//! +0                  FirmwareHeader (64 bytes)
//! +64                 firmware image (`header.image_len` bytes)
//! ...                 unused erased space
//! slot_size - 64      Ed25519 signature
//! ```
//!
//! SHA-256 covers the image.  Ed25519 covers the serialized header followed
//! by the image, so neither the version nor the image length or key id can be
//! changed without invalidating the signature.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use caplet_crypto::{ed25519_verify, sha256, ED25519_PUBLIC_KEY_LEN};
use caplet_security::flash::FlashRegion;

use crate::flash::{FlashIoError, FlashStorage};

/// Serialized firmware header size.
pub const HEADER_SIZE: usize = 64;
/// Ed25519 signature trailer size.
pub const SIGNATURE_SIZE: usize = 64;
/// Minimum slot capacity for a header, empty image and signature.
pub const MIN_SLOT_SIZE: usize = HEADER_SIZE + SIGNATURE_SIZE;
/// Maximum number of trusted keys in a boot ROM trust store.
pub const MAX_TRUSTED_KEYS: usize = 8;

/// Errors returned by the verifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    /// Header magic is not `FWR1`.
    BadMagic,
    /// Slot is too small for its header and signature.
    ImageTooSmall,
    /// Declared image length exceeds the slot capacity.
    ImageTooLarge,
    /// SHA-256 of the image does not match the header.
    HashMismatch,
    /// Signature is invalid for the message.
    BadSignature,
    /// No trusted key has the requested id.
    UnknownKey,
    /// Flash read failed.
    Flash(FlashIoError),
}

impl From<FlashIoError> for VerifyError {
    fn from(value: FlashIoError) -> Self {
        VerifyError::Flash(value)
    }
}

/// Firmware manifest header stored at the start of each firmware slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct FirmwareHeader {
    /// Magic = `0x4657_5231` (`FWR1`).
    pub magic: u32,
    /// Packed semantic version (`major << 16 | minor << 8 | patch`).
    pub version: u32,
    /// Number of image bytes following the header.
    pub image_len: u32,
    /// Id of the Ed25519 key that signed this firmware.
    pub public_key_id: u32,
    /// SHA-256 of the image bytes only.
    pub hash: [u8; 32],
    /// Reserved; must be zero for version 1 firmware.
    pub reserved: [u8; 16],
}

impl FirmwareHeader {
    /// Magic identifying the firmware header format.
    pub const HEADER_MAGIC: u32 = 0x4657_5231; // 'FWR1'
    /// Offset of the image from the start of the slot.
    pub const IMAGE_OFFSET: u32 = HEADER_SIZE as u32;

    /// Build a header for an image.
    pub fn new(version: u32, image: &[u8], public_key_id: u32) -> Self {
        Self {
            magic: Self::HEADER_MAGIC,
            version,
            image_len: image.len() as u32,
            public_key_id,
            hash: sha256(image),
            reserved: [0u8; 16],
        }
    }

    /// Whether the magic is valid.
    pub const fn is_valid(&self) -> bool {
        self.magic == Self::HEADER_MAGIC
    }

    /// Serialize the header into exactly [`HEADER_SIZE`] bytes.
    pub fn to_bytes(self) -> [u8; HEADER_SIZE] {
        let mut out = [0u8; HEADER_SIZE];
        out[0..4].copy_from_slice(&self.magic.to_le_bytes());
        out[4..8].copy_from_slice(&self.version.to_le_bytes());
        out[8..12].copy_from_slice(&self.image_len.to_le_bytes());
        out[12..16].copy_from_slice(&self.public_key_id.to_le_bytes());
        out[16..48].copy_from_slice(&self.hash);
        out[48..64].copy_from_slice(&self.reserved);
        out
    }

    /// Parse a header from at least [`HEADER_SIZE`] bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, VerifyError> {
        if bytes.len() < HEADER_SIZE {
            return Err(VerifyError::ImageTooSmall);
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes[16..48]);
        let mut reserved = [0u8; 16];
        reserved.copy_from_slice(&bytes[48..64]);
        let header = Self {
            magic: u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            version: u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            image_len: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            public_key_id: u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            hash,
            reserved,
        };
        if !header.is_valid() {
            return Err(VerifyError::BadMagic);
        }
        Ok(header)
    }
}

const _: () = assert!(core::mem::size_of::<FirmwareHeader>() == HEADER_SIZE);

/// One trusted Ed25519 key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedKey {
    /// Key identifier stored in firmware headers.
    pub id: u32,
    /// Raw Ed25519 public key.
    pub public_key: [u8; ED25519_PUBLIC_KEY_LEN],
}

/// Fixed-capacity trust store suitable for a bootloader.
#[derive(Debug, Clone)]
pub struct TrustStore {
    keys: [Option<TrustedKey>; MAX_TRUSTED_KEYS],
}

impl Default for TrustStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TrustStore {
    /// Construct an empty trust store.
    pub const fn new() -> Self {
        Self {
            keys: [None; MAX_TRUSTED_KEYS],
        }
    }

    /// Add a trusted key.  Returns `false` when the store is full.
    pub fn add(&mut self, id: u32, public_key: [u8; ED25519_PUBLIC_KEY_LEN]) -> bool {
        let key = TrustedKey { id, public_key };
        for slot in &mut self.keys {
            if slot.is_none() {
                *slot = Some(key);
                return true;
            }
        }
        false
    }

    /// Find a key by id.
    pub fn get(&self, id: u32) -> Option<&TrustedKey> {
        self.keys.iter().flatten().find(|key| key.id == id)
    }

    /// Number of installed keys.
    pub fn len(&self) -> usize {
        self.keys.iter().flatten().count()
    }

    /// Whether no keys are installed.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Firmware verifier.
pub struct Verifier;

impl Verifier {
    /// Construct a verifier.
    pub const fn new() -> Self {
        Self
    }

    /// Verify a complete in-memory slot.
    ///
    /// `slot` must include the full reserved capacity of the slot because the
    /// signature trailer is addressed from the end.
    pub fn verify_slice(
        &self,
        slot: &[u8],
        keys: &TrustStore,
    ) -> Result<FirmwareHeader, VerifyError> {
        if slot.len() < MIN_SLOT_SIZE {
            return Err(VerifyError::ImageTooSmall);
        }
        let header = FirmwareHeader::from_bytes(slot)?;
        let image_len = header.image_len as usize;
        if image_len > slot.len() - MIN_SLOT_SIZE {
            return Err(VerifyError::ImageTooLarge);
        }
        let image = &slot[HEADER_SIZE..HEADER_SIZE + image_len];
        let sig_start = slot.len() - SIGNATURE_SIZE;
        let signature: &[u8; SIGNATURE_SIZE] = slot[sig_start..]
            .try_into()
            .map_err(|_| VerifyError::ImageTooSmall)?;
        self.verify_parts(&header, image, signature, keys)
    }

    /// Verify a slot directly from flash without trusting flash-mapped host
    /// memory.  The only allocation is the contiguous signed message required
    /// by Ed25519.
    pub fn verify_flash<S: FlashStorage>(
        &self,
        flash: &mut S,
        slot: FlashRegion,
        keys: &TrustStore,
    ) -> Result<FirmwareHeader, VerifyError> {
        let capacity = slot
            .end
            .checked_sub(slot.start)
            .and_then(|v| v.checked_add(1))
            .ok_or(VerifyError::ImageTooSmall)? as usize;
        if capacity < MIN_SLOT_SIZE {
            return Err(VerifyError::ImageTooSmall);
        }

        let mut header_bytes = [0u8; HEADER_SIZE];
        flash.read(slot.start, &mut header_bytes)?;
        let header = FirmwareHeader::from_bytes(&header_bytes)?;
        let image_len = header.image_len as usize;
        if image_len > capacity - MIN_SLOT_SIZE {
            return Err(VerifyError::ImageTooLarge);
        }

        let mut image = vec![0u8; image_len];
        flash.read(slot.start + HEADER_SIZE as u32, &mut image)?;

        let sig_addr = slot.end + 1 - SIGNATURE_SIZE as u32;
        let mut signature = [0u8; SIGNATURE_SIZE];
        flash.read(sig_addr, &mut signature)?;
        self.verify_parts(&header, &image, &signature, keys)
    }

    /// Verify parsed header, image and signature.
    pub fn verify_parts(
        &self,
        header: &FirmwareHeader,
        image: &[u8],
        signature: &[u8; SIGNATURE_SIZE],
        keys: &TrustStore,
    ) -> Result<FirmwareHeader, VerifyError> {
        if !header.is_valid() {
            return Err(VerifyError::BadMagic);
        }
        if header.image_len as usize != image.len() {
            return Err(VerifyError::ImageTooLarge);
        }
        if sha256(image) != header.hash {
            return Err(VerifyError::HashMismatch);
        }
        let key = keys
            .get(header.public_key_id)
            .ok_or(VerifyError::UnknownKey)?;

        let mut message = Vec::with_capacity(HEADER_SIZE + image.len());
        message.extend_from_slice(&header.to_bytes());
        message.extend_from_slice(image);
        if !ed25519_verify(&key.public_key, &message, signature) {
            return Err(VerifyError::BadSignature);
        }
        Ok(*header)
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;

    fn signed_slot(image: &[u8], version: u32) -> (Vec<u8>, TrustStore) {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let public = key.verifying_key().to_bytes();
        let header = FirmwareHeader::new(version, image, 7);
        let mut slot = vec![0xFFu8; 4096];
        slot[..HEADER_SIZE].copy_from_slice(&header.to_bytes());
        slot[HEADER_SIZE..HEADER_SIZE + image.len()].copy_from_slice(image);
        let mut message = Vec::new();
        message.extend_from_slice(&header.to_bytes());
        message.extend_from_slice(image);
        let sig = key.sign(&message).to_bytes();
        let sig_start = slot.len() - SIGNATURE_SIZE;
        slot[sig_start..].copy_from_slice(&sig);
        let mut keys = TrustStore::new();
        assert!(keys.add(7, public));
        (slot, keys)
    }

    #[test]
    fn valid_slot_verifies() {
        let (slot, keys) = signed_slot(b"firmware payload", 0x0001_0203);
        let header = Verifier::new().verify_slice(&slot, &keys).unwrap();
        assert_eq!(header.version, 0x0001_0203);
        assert_eq!(header.image_len, 16);
    }

    #[test]
    fn image_tampering_is_rejected() {
        let (mut slot, keys) = signed_slot(b"firmware payload", 1);
        slot[HEADER_SIZE + 2] ^= 1;
        assert_eq!(
            Verifier::new().verify_slice(&slot, &keys),
            Err(VerifyError::HashMismatch)
        );
    }

    #[test]
    fn header_tampering_breaks_signature() {
        let (mut slot, keys) = signed_slot(b"firmware payload", 1);
        slot[4] ^= 1;
        assert_eq!(
            Verifier::new().verify_slice(&slot, &keys),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn unknown_key_is_rejected() {
        let (slot, _) = signed_slot(b"firmware payload", 1);
        let keys = TrustStore::new();
        assert_eq!(
            Verifier::new().verify_slice(&slot, &keys),
            Err(VerifyError::UnknownKey)
        );
    }

    #[test]
    fn oversized_image_is_rejected() {
        let (mut slot, keys) = signed_slot(b"x", 1);
        slot[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            Verifier::new().verify_slice(&slot, &keys),
            Err(VerifyError::ImageTooLarge)
        );
    }

    #[test]
    fn header_has_stable_size_and_image_offset() {
        assert_eq!(core::mem::size_of::<FirmwareHeader>(), HEADER_SIZE);
        assert_eq!(FirmwareHeader::IMAGE_OFFSET as usize, HEADER_SIZE);
        assert_eq!(
            FirmwareHeader::new(1, b"abc", 2).to_bytes()[0..4],
            FirmwareHeader::HEADER_MAGIC.to_le_bytes()
        );
    }
}
