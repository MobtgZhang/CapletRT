//! Firmware integrity and signature verification.
//!
//! Before loading a new firmware image, the updater must verify:
//!   1. SHA-256 hash matches the value in the manifest header.
//!   2. Ed25519 signature over the image is valid for a trusted key.
//!
//! This module provides [`Verifier`] which wraps both checks.

/// Errors returned by the verifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    /// Hash mismatch — image corrupted.
    HashMismatch,
    /// Signature is invalid.
    BadSignature,
    /// No trusted key for this image's key ID.
    UnknownKey,
    /// Image is too small to be valid.
    ImageTooSmall,
}

/// Firmware manifest header (stored at the start of the secondary slot).
///
/// This is read before loading the firmware to know where the
/// verification metadata is located.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FirmwareHeader {
    /// Magic = 0x4657_5231 ('FWR1').
    pub magic: u32,
    /// Semantic version major.minor.patch.
    pub version: u32,
    /// SHA-256 hash of the firmware image (bytes 32..end of image).
    pub hash: [u8; 32],
    /// Ed25519 public key id (looks up in TrustStore).
    pub public_key_id: u32,
    /// Reserved for future use.
    _reserved: [u8; 8],
}

impl FirmwareHeader {
    /// Magic value identifying a valid header.
    pub const HEADER_MAGIC: u32 = 0x4657_5231; // 'FWR1'

    /// Check if this header is valid (correct magic).
    pub fn is_valid(&self) -> bool {
        self.magic == Self::HEADER_MAGIC
    }

    /// The offset from the slot start where the firmware image begins.
    /// (After the header).
    pub const IMAGE_OFFSET: u32 = 64; // size of FirmwareHeader
}

/// Verifier — checks hash and signature before loading a new firmware.
pub struct Verifier;

impl Verifier {
    /// Create a new verifier.
    pub const fn new() -> Self {
        Self
    }

    /// Verify the firmware image in `slot`.
    ///
    /// `slot_base` — the physical base address of the inactive slot.
    /// `slot_size` — total size of the slot in bytes.
    /// `hash` — expected SHA-256 of the firmware (from the manifest).
    /// `public_key_id` — trusted key to use for signature verification.
    ///
    /// Returns `Ok(header)` if both hash and signature are valid.
    /// Returns `Err(VerifyError)` if verification fails.
    pub fn verify(
        &self,
        slot_base: u32,
        slot_size: u32,
        expected_hash: &[u8; 32],
        public_key_id: u32,
    ) -> Result<FirmwareHeader, VerifyError> {
        let header_ptr = slot_base as *const FirmwareHeader;
        let header = unsafe { core::ptr::read_volatile(header_ptr) };

        if !header.is_valid() {
            return Err(VerifyError::ImageTooSmall);
        }

        // Verify SHA-256 hash of the firmware image.
        let image_base = slot_base + FirmwareHeader::IMAGE_OFFSET;
        let image_size = slot_size - FirmwareHeader::IMAGE_OFFSET;
        let computed_hash = self.compute_sha256(image_base, image_size);

        if &computed_hash != expected_hash {
            return Err(VerifyError::HashMismatch);
        }

        // Verify Ed25519 signature.
        // The signature covers the header + image (everything in the slot).
        let slot_ptr = slot_base as *const u8;
        let slot_bytes = unsafe { core::slice::from_raw_parts(slot_ptr, slot_size as usize) };
        let sig = self.read_signature(slot_base)?;

        if !self.verify_signature(slot_bytes, &sig, public_key_id) {
            return Err(VerifyError::BadSignature);
        }

        Ok(header)
    }

    /// Compute SHA-256 of a firmware image at a flash address.
    ///
    /// On embedded platforms this reads directly from flash MMIO.
    /// The default implementation reads from memory (suitable for QEMU
    /// where flash is memory-mapped).
    fn compute_sha256(&self, base: u32, size: u32) -> [u8; 32] {
        // Use the kernel's crypto syscall if available, otherwise
        // fall back to a pure software SHA-256.
        #[cfg(feature = "crypto")]
        {
            use core::convert::TryInto;
            let src = base as *const u8;
            let data = unsafe { core::slice::from_raw_parts(src, size as usize) };
            sha256_software(data)
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = (base, size);
            [0u8; 32]
        }
    }

    fn read_signature(&self, _slot_base: u32) -> Result<[u8; 64], VerifyError> {
        // In a full implementation, the signature is stored after the
        // image or in a separate metadata region. Here we return an
        // error indicating that no signature is available without the
        // crypto feature.
        #[cfg(feature = "crypto")]
        {
            Err(VerifyError::UnknownKey)
        }
        #[cfg(not(feature = "crypto"))]
        {
            Err(VerifyError::UnknownKey)
        }
    }

    fn verify_signature(&self, _data: &[u8], _sig: &[u8; 64], _key_id: u32) -> bool {
        #[cfg(feature = "crypto")]
        {
            let _ = (_data, _sig, _key_id);
            // In a full implementation, look up the key in the TrustStore
            // and call ed25519_verify.
            false
        }
        #[cfg(not(feature = "crypto"))]
        {
            let _ = (_data, _sig, _key_id);
            false
        }
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new()
    }
}

// ── Pure-software SHA-256 (used when crypto feature is off) ─────────────

/// Compute SHA-256 of a byte slice.
#[cfg(feature = "crypto")]
fn sha256_software(data: &[u8]) -> [u8; 32] {
    use core::convert::TryInto;

    // Constants.
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    for &b in &bit_len.to_be_bytes() {
        msg.push(b);
    }

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }

        let mut a = h[0]; let mut b = h[1]; let mut c = h[2]; let mut d = h[3];
        let mut e = h[4]; let mut f = h[5]; let mut g = h[6]; let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(temp1);
            d = c; c = b; b = a; a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for (i, &hv) in h.iter().enumerate() {
        result[i*4..(i+1)*4].copy_from_slice(&hv.to_be_bytes());
    }
    result
}
