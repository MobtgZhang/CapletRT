//! `no_std` cryptographic primitives used by CapletRT.
//!
//! This crate intentionally exposes a small, policy-free API.  Capability
//! checks and key allow-lists belong to the caller; the primitives here use
//! the RustCrypto/ed25519-dalek implementations rather than home-grown curve
//! arithmetic.

#![no_std]
#![deny(missing_docs)]
#![allow(clippy::chunks_exact_to_as_chunks)]

use aes::cipher::{BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use aes::Aes128;

/// Length of an Ed25519 public key in bytes.
pub const ED25519_PUBLIC_KEY_LEN: usize = 32;
/// Length of an Ed25519 signature in bytes.
pub const ED25519_SIGNATURE_LEN: usize = 64;
/// Length of an AES-128 key and block in bytes.
pub const AES128_BLOCK_LEN: usize = 16;

fn aes_block_aligned(data: &[u8]) -> bool {
    // AES128_BLOCK_LEN is a power of two.  Keep this compatible with the
    // workspace MSRV (1.75), where `usize::is_multiple_of` is unavailable.
    data.len() & (AES128_BLOCK_LEN - 1) == 0
}

/// Compute SHA-256 over `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Verify an Ed25519 signature.
///
/// `verify_strict` rejects non-canonical signatures and small-order public
/// keys, which is the appropriate policy for firmware/module authentication.
pub fn ed25519_verify(
    public_key: &[u8; ED25519_PUBLIC_KEY_LEN],
    message: &[u8],
    signature: &[u8; ED25519_SIGNATURE_LEN],
) -> bool {
    use ed25519_dalek::{Signature, VerifyingKey};

    let Ok(key) = VerifyingKey::from_bytes(public_key) else {
        return false;
    };
    let sig = Signature::from_bytes(signature);
    key.verify_strict(message, &sig).is_ok()
}

/// AES-128 ECB encryption of a block-aligned buffer.
///
/// Returns `false` without modifying the buffer when its length is not a
/// multiple of 16 bytes.
pub fn aes128_ecb_encrypt(key: &[u8; 16], data: &mut [u8]) -> bool {
    if !aes_block_aligned(data) {
        return false;
    }
    let cipher = Aes128::new_from_slice(key).expect("AES-128 key is 16 bytes");
    for block in data.chunks_exact_mut(AES128_BLOCK_LEN) {
        let mut lane = [0u8; AES128_BLOCK_LEN];
        lane.copy_from_slice(block);
        cipher.encrypt_block((&mut lane).into());
        block.copy_from_slice(&lane);
    }
    true
}

/// AES-128 ECB decryption of a block-aligned buffer.
///
/// Returns `false` without modifying the buffer when its length is not a
/// multiple of 16 bytes.
pub fn aes128_ecb_decrypt(key: &[u8; 16], data: &mut [u8]) -> bool {
    if !aes_block_aligned(data) {
        return false;
    }
    let cipher = Aes128::new_from_slice(key).expect("AES-128 key is 16 bytes");
    for block in data.chunks_exact_mut(AES128_BLOCK_LEN) {
        let mut lane = [0u8; AES128_BLOCK_LEN];
        lane.copy_from_slice(block);
        cipher.decrypt_block((&mut lane).into());
        block.copy_from_slice(&lane);
    }
    true
}

/// AES-128 CBC encryption without padding.
///
/// Returns `false` without modifying the buffer when its length is not a
/// multiple of 16 bytes.
pub fn aes128_cbc_encrypt(key: &[u8; 16], iv: &[u8; 16], data: &mut [u8]) -> bool {
    if !aes_block_aligned(data) {
        return false;
    }
    let cipher = Aes128::new_from_slice(key).expect("AES-128 key is 16 bytes");
    let mut chain = *iv;
    for block in data.chunks_exact_mut(AES128_BLOCK_LEN) {
        for (byte, previous) in block.iter_mut().zip(chain.iter()) {
            *byte ^= *previous;
        }
        let mut lane = [0u8; AES128_BLOCK_LEN];
        lane.copy_from_slice(block);
        cipher.encrypt_block((&mut lane).into());
        block.copy_from_slice(&lane);
        chain.copy_from_slice(block);
    }
    true
}

/// AES-128 CBC decryption without padding.
///
/// Returns `false` without modifying the buffer when its length is not a
/// multiple of 16 bytes.
pub fn aes128_cbc_decrypt(key: &[u8; 16], iv: &[u8; 16], data: &mut [u8]) -> bool {
    if !aes_block_aligned(data) {
        return false;
    }
    let cipher = Aes128::new_from_slice(key).expect("AES-128 key is 16 bytes");
    let mut chain = *iv;
    for block in data.chunks_exact_mut(AES128_BLOCK_LEN) {
        let ciphertext: [u8; 16] = block.try_into().expect("16-byte block");
        let mut lane = [0u8; AES128_BLOCK_LEN];
        lane.copy_from_slice(block);
        cipher.decrypt_block((&mut lane).into());
        block.copy_from_slice(&lane);
        for (byte, previous) in block.iter_mut().zip(chain.iter()) {
            *byte ^= *previous;
        }
        chain = ciphertext;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_nist_vector() {
        let digest = sha256(b"abc");
        assert_eq!(
            digest,
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ]
        );
    }

    #[test]
    fn aes128_ecb_nist_vector() {
        // FIPS-197 Appendix B.
        let key = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ];
        let mut block = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        assert!(aes128_ecb_encrypt(&key, &mut block));
        assert_eq!(
            block,
            [
                0x69, 0xc4, 0xe0, 0xd8, 0x6a, 0x7b, 0x04, 0x30, 0xd8, 0xcd, 0xb7, 0x80, 0x70, 0xb4,
                0xc5, 0x5a,
            ]
        );
        assert!(aes128_ecb_decrypt(&key, &mut block));
        assert_eq!(
            block,
            [
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff,
            ]
        );
    }

    #[test]
    fn aes128_cbc_round_trip() {
        let key = [0x2b; 16];
        let iv = [0x19; 16];
        let original = [0x5au8; 48];
        let mut data = original;
        assert!(aes128_cbc_encrypt(&key, &iv, &mut data));
        assert_ne!(data, original);
        assert!(aes128_cbc_decrypt(&key, &iv, &mut data));
        assert_eq!(data, original);
    }

    #[test]
    fn rejects_partial_blocks() {
        let key = [0u8; 16];
        let iv = [0u8; 16];
        let mut data = [0u8; 15];
        assert!(!aes128_ecb_encrypt(&key, &mut data));
        assert!(!aes128_cbc_encrypt(&key, &iv, &mut data));
    }
}
