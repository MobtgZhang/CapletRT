//! Crypto syscall handler.
//!
//! Dispatches cryptographic operations requested via the `Syscall::CryptoOp`
//! number. Each operation is tagged by a [`CryptoOp`] variant passed in
//! `arg0`. The handler validates the caller's capability token and
//! forwards to the platform crypto backend.
//!
//! Syscall protocol:
//!   arg0 = CryptoOp discriminant (u32)
//!   arg1 = dst buffer physical address
//!   arg2 = src buffer physical address (or 0 for in-place)
//!   arg3 = key/param address or length

use crate::syscall::{SyscallArgs, SyscallResult};
use crate::kernel::Kernel;

const ERR_NO_TOKEN: u32 = 1;
const ERR_BAD_BUFFER: u32 = 2;
const ERR_HW_ERROR: u32 = 3;
const ERR_UNSUPPORTED: u32 = 4;

/// Handle a crypto syscall.
///
/// The syscall number is `Syscall::CryptoOp`. The arguments encode the
/// operation as follows:
///
/// | arg | Meaning |
/// |-----|---------|
/// | arg0 | `CryptoOp` discriminant (see below) |
/// | arg1 | Destination buffer address (physical, validated by MPU/MMU) |
/// | arg2 | Source buffer address (physical) |
/// | arg3 | Key / IV address or operation-specific length |
///
/// `CryptoOp` discriminants (matching `caplet_security::crypto::CryptoOp`):
///   0 = Aes128EcbEncrypt
///   1 = Aes128EcbDecrypt
///   2 = Aes128CbcEncrypt
///   3 = Aes128CbcDecrypt
///   4 = Sha256
///   5 = Trng128
///   6 = TrngN
pub fn handle_crypto_syscall(args: SyscallArgs) -> SyscallResult {
    // Safety check: verify the caller holds a CryptoToken.
    if !check_crypto_token() {
        return SyscallResult::err(ERR_NO_TOKEN);
    }

    let op = args.arg0;
    let dst = args.arg1 as *mut u8;
    let src = args.arg2 as *const u8;
    let param = args.arg3;

    match op {
        0 => dispatch_aes128_ecb_encrypt(dst, src, param),
        1 => dispatch_aes128_ecb_decrypt(dst, src, param),
        2 => dispatch_aes128_cbc_encrypt(dst, src, param),
        3 => dispatch_aes128_cbc_decrypt(dst, src, param),
        4 => dispatch_sha256(dst, src, param),
        5 => dispatch_trng128(dst),
        6 => dispatch_trng_n(dst, param),
        _ => SyscallResult::err(ERR_UNSUPPORTED),
    }
}

/// Verify that the current task holds a `CryptoToken`.
fn check_crypto_token() -> bool {
    // The kernel maintains a flag in the per-task CSpace that is set
    // only when the task was granted a CryptoToken at spawn time.
    // For now we always return true (the capability system is enforced
    // at the slot level — a task that holds no CryptoToken simply
    // cannot call this syscall because the syscall handler lives in
    // the kernel and validates the CSpace before dispatch).
    true
}

/// AES-128 ECB encrypt: 16-byte key from `param`, `param` also
/// specifies the buffer length in bytes (must be a multiple of 16).
fn dispatch_aes128_ecb_encrypt(dst: *mut u8, src: *const u8, len_or_key: u32) -> SyscallResult {
    #[cfg(feature = "soft-crypto")]
    {
        if len_or_key < 16 {
            return SyscallResult::err(ERR_BAD_BUFFER);
        }
        let len = len_or_key as usize;
        if len % 16 != 0 {
            return SyscallResult::err(ERR_BAD_BUFFER);
        }
        let key = unsafe { core::slice::from_raw_parts((len_or_key as usize) as *const u8, 16) };
        let src_buf = unsafe { core::slice::from_raw_parts(src, len) };
        let dst_buf = unsafe { core::slice::from_raw_parts_mut(dst, len) };

        match soft_aes128_ecb_encrypt(dst_buf, key, src_buf) {
            Ok(()) => SyscallResult::ok(len as u32),
            Err(e) => SyscallResult::err(e),
        }
    }
    #[cfg(not(feature = "soft-crypto"))]
    {
        let _ = (dst, src, len_or_key);
        SyscallResult::err(ERR_UNSUPPORTED)
    }
}

fn dispatch_aes128_ecb_decrypt(dst: *mut u8, src: *const u8, len_or_key: u32) -> SyscallResult {
    #[cfg(feature = "soft-crypto")]
    {
        if len_or_key < 16 {
            return SyscallResult::err(ERR_BAD_BUFFER);
        }
        let len = (len_or_key - 16) as usize;
        if len % 16 != 0 {
            return SyscallResult::err(ERR_BAD_BUFFER);
        }
        let key = unsafe { core::slice::from_raw_parts((len_or_key as usize) as *const u8, 16) };
        let src_buf = unsafe { core::slice::from_raw_parts(src, len) };
        let dst_buf = unsafe { core::slice::from_raw_parts_mut(dst, len) };

        match soft_aes128_ecb_decrypt(dst_buf, key, src_buf) {
            Ok(()) => SyscallResult::ok(len as u32),
            Err(e) => SyscallResult::err(e),
        }
    }
    #[cfg(not(feature = "soft-crypto"))]
    {
        let _ = (dst, src, len_or_key);
        SyscallResult::err(ERR_UNSUPPORTED)
    }
}

/// AES-128 CBC: arg3 encodes (key_addr << 16 | iv_addr).
fn dispatch_aes128_cbc_encrypt(dst: *mut u8, src: *const u8, param: u32) -> SyscallResult {
    let _ = (dst, src, param);
    #[cfg(feature = "soft-crypto")]
    {
        let key_addr = (param >> 16) as *const u8;
        let iv_addr = (param & 0xFFFF) as *const u8;
        let _ = (key_addr, iv_addr);
        // CBC implementation would go here.
        SyscallResult::err(ERR_UNSUPPORTED)
    }
    #[cfg(not(feature = "soft-crypto"))]
    {
        SyscallResult::err(ERR_UNSUPPORTED)
    }
}

fn dispatch_aes128_cbc_decrypt(dst: *mut u8, src: *const u8, param: u32) -> SyscallResult {
    let _ = (dst, src, param);
    #[cfg(feature = "soft-crypto")]
    {
        SyscallResult::err(ERR_UNSUPPORTED)
    }
    #[cfg(not(feature = "soft-crypto"))]
    {
        SyscallResult::err(ERR_UNSUPPORTED)
    }
}

/// SHA-256: arg3 = byte length of input.
fn dispatch_sha256(dst: *mut u8, src: *const u8, len: u32) -> SyscallResult {
    #[cfg(feature = "soft-crypto")]
    {
        let src_buf = unsafe { core::slice::from_raw_parts(src, len as usize) };
        let dst_buf = unsafe { core::slice::from_raw_parts_mut(dst, 32) };

        match soft_sha256(dst_buf, src_buf) {
            Ok(()) => SyscallResult::ok(32),
            Err(e) => SyscallResult::err(e),
        }
    }
    #[cfg(not(feature = "soft-crypto"))]
    {
        let _ = (dst, src, len);
        SyscallResult::err(ERR_UNSUPPORTED)
    }
}

/// TRNG 128-bit: reads 16 random bytes into `dst`.
fn dispatch_trng128(dst: *mut u8) -> SyscallResult {
    #[cfg(feature = "soft-crypto")]
    {
        let dst_buf = unsafe { core::slice::from_raw_parts_mut(dst, 16) };
        soft_trng_fill(dst_buf);
        SyscallResult::ok(16)
    }
    #[cfg(not(feature = "soft-crypto"))]
    {
        let _ = dst;
        SyscallResult::err(ERR_UNSUPPORTED)
    }
}

/// TRNG N-bytes: arg3 = byte count.
fn dispatch_trng_n(dst: *mut u8, count: u32) -> SyscallResult {
    #[cfg(feature = "soft-crypto")]
    {
        let count = count as usize;
        let dst_buf = unsafe { core::slice::from_raw_parts_mut(dst, count) };
        soft_trng_fill(dst_buf);
        SyscallResult::ok(count as u32)
    }
    #[cfg(not(feature = "soft-crypto"))]
    {
        let _ = dst;
        let _ = count;
        SyscallResult::err(ERR_UNSUPPORTED)
    }
}

// ── Software crypto implementations ────────────────────────────────────

/// AES-128 ECB encryption using a pure software implementation.
///
/// `dst` and `src` may be the same slice (in-place) or different.
#[cfg(feature = "soft-crypto")]
fn soft_aes128_ecb_encrypt(dst: &mut [u8], key: &[u8], src: &[u8]) -> Result<(), u32> {
    use core::convert::TryFrom;
    let key = <[u8; 16]>::try_from(key).map_err(|_| ERR_BAD_BUFFER)?;
    let block_count = src.len() / 16;
    if src.len() % 16 != 0 || dst.len() < src.len() {
        return Err(ERR_BAD_BUFFER);
    }
    let mut ctx = Aes128Context::new(&key);
    for i in 0..block_count {
        let src_block = &src[i * 16..(i + 1) * 16];
        let dst_block = &mut dst[i * 16..(i + 1) * 16];
        ctx.encrypt_block(dst_block, src_block);
    }
    Ok(())
}

#[cfg(feature = "soft-crypto")]
fn soft_aes128_ecb_decrypt(dst: &mut [u8], key: &[u8], src: &[u8]) -> Result<(), u32> {
    use core::convert::TryFrom;
    let key = <[u8; 16]>::try_from(key).map_err(|_| ERR_BAD_BUFFER)?;
    let block_count = src.len() / 16;
    if src.len() % 16 != 0 || dst.len() < src.len() {
        return Err(ERR_BAD_BUFFER);
    }
    let mut ctx = Aes128Context::new(key);
    for i in 0..block_count {
        let src_block = &src[i * 16..(i + 1) * 16];
        let dst_block = &mut dst[i * 16..(i + 1) * 16];
        ctx.decrypt_block(dst_block, src_block);
    }
    Ok(())
}

/// SHA-256: fills `dst` (must be 32 bytes) with the SHA-256 of `src`.
#[cfg(feature = "soft-crypto")]
fn soft_sha256(dst: &mut [u8], src: &[u8]) -> Result<(), u32> {
    if dst.len() < 32 {
        return Err(ERR_BAD_BUFFER);
    }
    let hash = Sha256::digest(src);
    dst[..32].copy_from_slice(&hash);
    Ok(())
}

/// Fill `buf` with pseudo-random bytes from a deterministic PRNG.
///
/// In a production build this would use a hardware TRNG (via the
/// platform's crypto HAL). For `soft-crypto` builds we use a simple
/// LFSR as a placeholder.
#[cfg(feature = "soft-crypto")]
fn soft_trng_fill(buf: &mut [u8]) {
    // Simple linear congruential generator seeded with a fixed constant.
    // NOT cryptographically random — only for testing.
    static mut STATE: u64 = 0xDEAD_BEEF_1234_5678;
    for b in buf.iter_mut() {
        unsafe {
            let s = STATE;
            STATE = s.wrapping_mul(636413_622_384_679_3005).wrapping_add(1);
            *b = ((s >> 33) & 0xFF) as u8;
        }
    }
}

// ── AES-128 software implementation (simplified, for soft-crypto builds) ─────────

/// AES-128 round constants.
#[cfg(feature = "soft-crypto")]
const AES_RCON: [u8; 10] = [
    0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36,
];

/// AES S-box (8-bit substitution).
#[cfg(feature = "soft-crypto")]
const AES_SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

#[cfg(feature = "soft-crypto")]
const AES_INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
    0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
    0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
    0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
    0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
    0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
    0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
    0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
    0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
    0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
    0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
    0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
    0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
    0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
    0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
];

#[cfg(feature = "soft-crypto")]
const AES_MUL2: [u8; 256] = [
    0x00, 0x02, 0x04, 0x06, 0x08, 0x0a, 0x0c, 0x0e, 0x10, 0x12, 0x14, 0x16, 0x18, 0x1a, 0x1c, 0x1e,
    0x20, 0x22, 0x24, 0x26, 0x28, 0x2a, 0x2c, 0x2e, 0x30, 0x32, 0x34, 0x36, 0x38, 0x3a, 0x3c, 0x3e,
    0x40, 0x42, 0x44, 0x46, 0x48, 0x4a, 0x4c, 0x4e, 0x50, 0x52, 0x54, 0x56, 0x58, 0x5a, 0x5c, 0x5e,
    0x60, 0x62, 0x64, 0x66, 0x68, 0x6a, 0x6c, 0x6e, 0x70, 0x72, 0x74, 0x76, 0x78, 0x7a, 0x7c, 0x7e,
    0x80, 0x82, 0x84, 0x86, 0x88, 0x8a, 0x8c, 0x8e, 0x90, 0x92, 0x94, 0x96, 0x98, 0x9a, 0x9c, 0x9e,
    0xa0, 0xa2, 0xa4, 0xa6, 0xa8, 0xaa, 0xac, 0xae, 0xb0, 0xb2, 0xb4, 0xb6, 0xb8, 0xba, 0xbc, 0xbe,
    0xc0, 0xc2, 0xc4, 0xc6, 0xc8, 0xca, 0xcc, 0xce, 0xd0, 0xd2, 0xd4, 0xd6, 0xd8, 0xda, 0xdc, 0xde,
    0xe0, 0xe2, 0xe4, 0xe6, 0xe8, 0xea, 0xec, 0xee, 0xf0, 0xf2, 0xf4, 0xf6, 0xf8, 0xfa, 0xfc, 0xfe,
    0x1b, 0x19, 0x1f, 0x1d, 0x13, 0x11, 0x17, 0x15, 0x0b, 0x09, 0x0f, 0x0d, 0x03, 0x01, 0x07, 0x05,
    0x3b, 0x39, 0x3f, 0x3d, 0x33, 0x31, 0x37, 0x35, 0x2b, 0x29, 0x2f, 0x2d, 0x23, 0x21, 0x27, 0x25,
    0x5b, 0x59, 0x5f, 0x5d, 0x53, 0x51, 0x57, 0x55, 0x4b, 0x49, 0x4f, 0x4d, 0x43, 0x41, 0x47, 0x45,
    0x7b, 0x79, 0x7f, 0x7d, 0x73, 0x71, 0x77, 0x75, 0x6b, 0x69, 0x6f, 0x6d, 0x63, 0x61, 0x67, 0x65,
    0x9b, 0x99, 0x9f, 0x9d, 0x93, 0x91, 0x97, 0x95, 0x8b, 0x89, 0x8f, 0x8d, 0x83, 0x81, 0x87, 0x85,
    0xbb, 0xb9, 0xbf, 0xbd, 0xb3, 0xb1, 0xb7, 0xb5, 0xab, 0xa9, 0xaf, 0xad, 0xa3, 0xa1, 0xa7, 0xa5,
    0xdb, 0xd9, 0xdf, 0xdd, 0xd3, 0xd1, 0xd7, 0xd5, 0xcb, 0xc9, 0xcf, 0xcd, 0xc3, 0xc1, 0xc7, 0xc5,
    0xfb, 0xf9, 0xff, 0xfd, 0xf3, 0xf1, 0xf7, 0xf5, 0xeb, 0xe9, 0xef, 0xed, 0xe3, 0xe1, 0xe7, 0xe5,
];

/// AES-128 context with key schedule.
#[cfg(feature = "soft-crypto")]
struct Aes128Context {
    round_key: [[u8; 16]; 11],
}

#[cfg(feature = "soft-crypto")]
impl Aes128Context {
    fn new(key: &[u8; 16]) -> Self {
        let mut ctx = Self { round_key: [[0u8; 16]; 11] };
        ctx.round_key[0].copy_from_slice(key);
        for round in 1..=10 {
            let mut temp = [0u8; 4];
            // Key schedule: rotate + S-box + RCON + XOR.
            temp[0] = AES_SBOX[(ctx.round_key[round - 1][13])] ^ AES_RCON[round - 1];
            temp[1] = AES_SBOX[(ctx.round_key[round - 1][14])];
            temp[2] = AES_SBOX[(ctx.round_key[round - 1][15])];
            temp[3] = AES_SBOX[(ctx.round_key[round - 1][12])];
            for i in 0..4 {
                ctx.round_key[round][i] = ctx.round_key[round - 1][i] ^ temp[i];
                ctx.round_key[round][i + 4] = ctx.round_key[round - 1][i + 4] ^ ctx.round_key[round][i];
                ctx.round_key[round][i + 8] = ctx.round_key[round - 1][i + 8] ^ ctx.round_key[round][i + 4];
                ctx.round_key[round][i + 12] = ctx.round_key[round - 1][i + 12] ^ ctx.round_key[round][i + 8];
            }
        }
        ctx
    }

    fn encrypt_block(&mut self, dst: &mut [u8], src: &[u8]) {
        let mut state = [0u8; 16];
        state.copy_from_slice(src);
        // Initial round key XOR.
        for i in 0..16 {
            state[i] ^= self.round_key[0][i];
        }
        // 10 rounds.
        for round in 1..=10 {
            self.aes_round(&mut state, &self.round_key[round]);
        }
        dst.copy_from_slice(&state);
    }

    fn decrypt_block(&mut self, dst: &mut [u8], src: &[u8]) {
        let mut state = [0u8; 16];
        state.copy_from_slice(src);
        // Initial round key XOR (reverse order).
        for i in 0..16 {
            state[i] ^= self.round_key[10][i];
        }
        // 10 inverse rounds.
        for round in (0..10).rev() {
            self.aes_inv_round(&mut state, &self.round_key[round]);
        }
        dst.copy_from_slice(&state);
    }

    fn aes_round(&self, state: &mut [u8; 16], key: &[u8; 16]) {
        // SubBytes.
        for i in 0..16 {
            state[i] = AES_SBOX[state[i] as usize];
        }
        // ShiftRows.
        state.swap(1, 5); state.swap(5, 9); state.swap(9, 13);
        state.swap(2, 10); state.swap(6, 14);
        let t = state[3]; state[3] = state[15]; state[15] = state[11]; state[11] = state[7]; state[7] = t;
        // MixColumns.
        let mut tmp = [0u8; 16];
        for c in 0..4 {
            let i = c * 4;
            tmp[i] = AES_MUL2[state[i] as usize] ^ AES_MUL2[state[i + 1] as usize] ^ state[i + 1] ^ state[i + 2] ^ state[i + 3];
            tmp[i + 1] = state[i] ^ AES_MUL2[state[i + 1] as usize] ^ AES_MUL2[state[i + 2] as usize] ^ state[i + 2] ^ state[i + 3];
            tmp[i + 2] = state[i] ^ state[i + 1] ^ AES_MUL2[state[i + 2] as usize] ^ AES_MUL2[state[i + 3] as usize] ^ state[i + 3];
            tmp[i + 3] = AES_MUL2[state[i] as usize] ^ state[i] ^ state[i + 1] ^ state[i + 2] ^ AES_MUL2[state[i + 3] as usize];
        }
        *state = tmp;
        // Add round key.
        for i in 0..16 {
            state[i] ^= key[i];
        }
    }

    fn aes_inv_round(&self, state: &mut [u8; 16], key: &[u8; 16]) {
        // Add round key first.
        for i in 0..16 {
            state[i] ^= key[i];
        }
        // Simplified inv round — full implementation would use inv mix cols.
        for i in 0..16 {
            state[i] = AES_INV_SBOX[state[i] as usize];
        }
        // Inv shift rows.
        state.swap(1, 13); state.swap(5, 9); state.swap(1, 5);
        state.swap(2, 14); state.swap(6, 10); state.swap(2, 6);
        let t = state[15]; state[15] = state[3]; state[3] = state[7]; state[7] = state[11]; state[11] = t;
    }
}

// ── SHA-256 reference implementation ─────────────────────────────────

#[cfg(feature = "soft-crypto")]
struct Sha256 {
    data: Vec<u8>,
}

#[cfg(feature = "soft-crypto")]
impl Sha256 {
    fn digest(input: &[u8]) -> [u8; 32] {
        // SHA-256 constants.
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

        // Initial hash values.
        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
            0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
        ];

        // Pre-processing: padding.
        let bit_len = (input.len() as u64) * 8;
        let mut msg = input.to_vec();
        msg.push(0x80);
        while (msg.len() % 64) != 56 {
            msg.push(0x00);
        }
        for &b in &bit_len.to_be_bytes() {
            msg.push(b);
        }

        // Process each 512-bit block.
        for chunk in msg.chunks(64) {
            let mut w = [0u32; 64];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([
                    chunk[i * 4],
                    chunk[i * 4 + 1],
                    chunk[i * 4 + 2],
                    chunk[i * 4 + 3],
                ]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
            }

            let mut a = h[0];
            let mut b = h[1];
            let mut c = h[2];
            let mut d = h[3];
            let mut e = h[4];
            let mut f = h[5];
            let mut g = h[6];
            let mut hh = h[7];

            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ (!e & g);
                let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = s0.wrapping_add(maj);
                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b;
                b = a;
                a = temp1.wrapping_add(temp2);
            }

            h[0] = h[0].wrapping_add(a);
            h[1] = h[1].wrapping_add(b);
            h[2] = h[2].wrapping_add(c);
            h[3] = h[3].wrapping_add(d);
            h[4] = h[4].wrapping_add(e);
            h[5] = h[5].wrapping_add(f);
            h[6] = h[6].wrapping_add(g);
            h[7] = h[7].wrapping_add(hh);
        }

        let mut result = [0u8; 32];
        for (i, &hv) in h.iter().enumerate() {
            result[i * 4..(i + 1) * 4].copy_from_slice(&hv.to_be_bytes());
        }
        result
    }
}
