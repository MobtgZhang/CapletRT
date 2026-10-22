//! Crypto syscall handler.
//!
//! Dispatches cryptographic operations requested through
//! [`Syscall::CryptoOp`](crate::syscall::Syscall::CryptoOp).  The caller must
//! hold a live `crypto:0` capability with read permission.  Key material is
//! never copied into the kernel heap: the primitives operate directly on the
//! caller-provided buffers.
//!
//! Syscall protocol (32-bit and 64-bit ABI):
//!
//! | arg | Meaning |
//! |-----|---------|
//! | arg0 | `CryptoOp` discriminant |
//! | arg1 | destination buffer pointer |
//! | arg2 | source buffer pointer |
//! | arg3 | pointer to [`CryptoParams`] |
//!
//! [`CryptoParams`] carries a byte length plus optional key/IV pointers.
//! `Trng128` ignores the length and produces 16 bytes; `TrngN` uses `len`.
//!
//! The operation returns the number of bytes written on success.
//! If no hardware/platform TRNG backend has been installed, `TRNG` requests
//! fail with [`ERR_UNSUPPORTED`] instead of returning predictable bytes.

use crate::security_grants::crypto_allowed;
use crate::syscall::{SyscallArgs, SyscallResult, WideSyscallArgs};
use caplet_crypto::{
    aes128_cbc_decrypt, aes128_cbc_encrypt, aes128_ecb_decrypt, aes128_ecb_encrypt, sha256,
    AES128_BLOCK_LEN,
};
use caplet_sync::SpinLock;

/// Caller does not hold a crypto capability.
pub const ERR_NO_TOKEN: u32 = 1;
/// A buffer or length is invalid for the operation.
pub const ERR_BAD_BUFFER: u32 = 2;
/// The crypto backend reported a hardware error.
pub const ERR_HW_ERROR: u32 = 3;
/// The requested operation is not available in this build/board.
pub const ERR_UNSUPPORTED: u32 = 4;

/// Operation-specific parameters passed through `arg3`.
///
/// The fields use pointer-sized integers so the structure has the same
/// layout as the native ABI on 32-bit and 64-bit targets.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CryptoParams {
    /// Number of bytes to process.
    pub len: usize,
    /// Pointer to the 16-byte AES key (unused by SHA-256 / TRNG).
    pub key: usize,
    /// Pointer to the 16-byte CBC IV (unused by ECB / SHA-256 / TRNG).
    pub iv: usize,
    /// Reserved flag word; must be zero.
    pub flags: u32,
}

impl CryptoParams {
    /// Construct a parameter block.
    pub const fn new(len: usize, key: usize, iv: usize) -> Self {
        Self {
            len,
            key,
            iv,
            flags: 0,
        }
    }
}

/// Fill `buf` with true random bytes.
///
/// A backend returns `false` when the entropy source failed.  Boards install
/// one with [`set_trng_backend`] during bring-up.  There is deliberately no
/// deterministic fallback: predictable "random" data is a security bug.
pub type TrngFillFn = fn(&mut [u8]) -> bool;

static TRNG_BACKEND: SpinLock<Option<TrngFillFn>> = SpinLock::new(None);

/// Install (or clear) the platform TRNG backend.
pub fn set_trng_backend(backend: Option<TrngFillFn>) {
    *TRNG_BACKEND.lock() = backend;
}

/// Handle a crypto syscall using the native 32-bit argument ABI.
pub fn handle_crypto_syscall(args: SyscallArgs) -> SyscallResult {
    dispatch_raw(
        args.arg0,
        args.arg1 as usize,
        args.arg2 as usize,
        args.arg3 as usize,
    )
}

/// Handle a crypto syscall with full-width pointers (64-bit hosts/targets).
pub fn handle_crypto_syscall_wide(args: WideSyscallArgs) -> SyscallResult {
    dispatch_raw(
        args.arg0,
        args.arg1_ptr::<u8>() as usize,
        args.arg2_ptr::<u8>() as usize,
        args.arg3_ptr::<CryptoParams>() as usize,
    )
}

fn dispatch_raw(op: u32, dst: usize, src: usize, param: usize) -> SyscallResult {
    if !crypto_allowed() {
        return SyscallResult::err(ERR_NO_TOKEN);
    }
    let Some(params) = read_params(param) else {
        return SyscallResult::err(ERR_BAD_BUFFER);
    };
    match op {
        0 => aes_ecb(dst, src, &params, true),
        1 => aes_ecb(dst, src, &params, false),
        2 => aes_cbc(dst, src, &params, true),
        3 => aes_cbc(dst, src, &params, false),
        4 => sha256_op(dst, src, params.len),
        5 => trng(dst, 16),
        6 => trng(dst, params.len),
        _ => SyscallResult::err(ERR_UNSUPPORTED),
    }
}

fn read_params(ptr: usize) -> Option<CryptoParams> {
    if ptr == 0 {
        return None;
    }
    // Parameter blocks originate in user memory and need not be naturally
    // aligned on every supported ABI.
    let params = unsafe { core::ptr::read_unaligned(ptr as *const CryptoParams) };
    if params.flags != 0 {
        return None;
    }
    Some(params)
}

fn key_from(ptr: usize) -> Option<[u8; AES128_BLOCK_LEN]> {
    if ptr == 0 {
        return None;
    }
    let mut key = [0u8; AES128_BLOCK_LEN];
    unsafe {
        core::ptr::copy_nonoverlapping(ptr as *const u8, key.as_mut_ptr(), AES128_BLOCK_LEN);
    }
    Some(key)
}

fn iv_from(ptr: usize) -> Option<[u8; AES128_BLOCK_LEN]> {
    key_from(ptr)
}

fn checked_len(len: usize) -> Option<usize> {
    if len == 0 || len % AES128_BLOCK_LEN != 0 {
        None
    } else {
        Some(len)
    }
}

fn aes_ecb(dst: usize, src: usize, params: &CryptoParams, encrypt: bool) -> SyscallResult {
    let Some(len) = checked_len(params.len) else {
        return SyscallResult::err(ERR_BAD_BUFFER);
    };
    if dst == 0 || src == 0 {
        return SyscallResult::err(ERR_BAD_BUFFER);
    }
    let Some(key) = key_from(params.key) else {
        return SyscallResult::err(ERR_BAD_BUFFER);
    };

    // `ptr::copy` is memmove semantics, so src == dst and overlapping buffers
    // are both supported.
    unsafe {
        core::ptr::copy(src as *const u8, dst as *mut u8, len);
        let buf = core::slice::from_raw_parts_mut(dst as *mut u8, len);
        let ok = if encrypt {
            aes128_ecb_encrypt(&key, buf)
        } else {
            aes128_ecb_decrypt(&key, buf)
        };
        if ok {
            SyscallResult::ok(len as u32)
        } else {
            SyscallResult::err(ERR_BAD_BUFFER)
        }
    }
}

fn aes_cbc(dst: usize, src: usize, params: &CryptoParams, encrypt: bool) -> SyscallResult {
    let Some(len) = checked_len(params.len) else {
        return SyscallResult::err(ERR_BAD_BUFFER);
    };
    if dst == 0 || src == 0 {
        return SyscallResult::err(ERR_BAD_BUFFER);
    }
    let Some(key) = key_from(params.key) else {
        return SyscallResult::err(ERR_BAD_BUFFER);
    };
    let Some(iv) = iv_from(params.iv) else {
        return SyscallResult::err(ERR_BAD_BUFFER);
    };

    unsafe {
        core::ptr::copy(src as *const u8, dst as *mut u8, len);
        let buf = core::slice::from_raw_parts_mut(dst as *mut u8, len);
        let ok = if encrypt {
            aes128_cbc_encrypt(&key, &iv, buf)
        } else {
            aes128_cbc_decrypt(&key, &iv, buf)
        };
        if ok {
            SyscallResult::ok(len as u32)
        } else {
            SyscallResult::err(ERR_BAD_BUFFER)
        }
    }
}

fn sha256_op(dst: usize, src: usize, len: usize) -> SyscallResult {
    if dst == 0 || (src == 0 && len != 0) {
        return SyscallResult::err(ERR_BAD_BUFFER);
    }
    let input: &[u8] = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(src as *const u8, len) }
    };
    let digest = sha256(input);
    unsafe {
        core::ptr::copy_nonoverlapping(digest.as_ptr(), dst as *mut u8, digest.len());
    }
    SyscallResult::ok(digest.len() as u32)
}

fn trng(dst: usize, len: usize) -> SyscallResult {
    if dst == 0 && len != 0 {
        return SyscallResult::err(ERR_BAD_BUFFER);
    }
    if len == 0 {
        return SyscallResult::ok(0);
    }
    if len > u32::MAX as usize {
        return SyscallResult::err(ERR_BAD_BUFFER);
    }
    let Some(backend) = *TRNG_BACKEND.lock() else {
        return SyscallResult::err(ERR_UNSUPPORTED);
    };
    let buf = unsafe { core::slice::from_raw_parts_mut(dst as *mut u8, len) };
    if backend(buf) {
        SyscallResult::ok(len as u32)
    } else {
        SyscallResult::err(ERR_HW_ERROR)
    }
}
