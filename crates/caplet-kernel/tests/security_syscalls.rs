//! End-to-end tests for the capability-gated crypto and flash syscalls.

use caplet_kernel::security_grants::{
    clear_security_grants, install_crypto_capability, install_flash_capability,
};
use caplet_kernel::syscall::{Syscall, WideSyscallArgs};
use caplet_kernel::syscall_crypto::{set_trng_backend, ERR_UNSUPPORTED};
use caplet_kernel::syscall_crypto::{CryptoParams, ERR_NO_TOKEN as CRYPTO_NO_TOKEN};
use caplet_kernel::syscall_dispatch::syscall_dispatch_wide;
use caplet_kernel::syscall_flash::{
    peek_flash_mirror, reset_flash_mirror, seed_flash_mirror, ERR_BAD_ALIGNMENT, ERR_NOT_ERASED,
    ERR_OUT_OF_REGION, ERR_WRITE_PROTECTED, FLASH_BASE, FLASH_SECTOR_SIZE,
};

fn call_crypto(
    op: u32,
    dst: &mut [u8],
    src: &[u8],
    params: &CryptoParams,
) -> caplet_kernel::syscall::SyscallResult {
    syscall_dispatch_wide(
        WideSyscallArgs::new(
            Syscall::CryptoOp.number(),
            op,
            dst.as_mut_ptr() as u64,
            src.as_ptr() as u64,
            0,
        )
        .with_arg3_ptr(params as *const CryptoParams),
    )
}

fn call_crypto_in_place(
    op: u32,
    buf: &mut [u8],
    params: &CryptoParams,
) -> caplet_kernel::syscall::SyscallResult {
    syscall_dispatch_wide(
        WideSyscallArgs::new(
            Syscall::CryptoOp.number(),
            op,
            buf.as_mut_ptr() as u64,
            buf.as_mut_ptr() as u64,
            0,
        )
        .with_arg3_ptr(params as *const CryptoParams),
    )
}

fn call_flash_read(offset: u32, dst: &mut [u8]) -> caplet_kernel::syscall::SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::FlashRead.number(),
        offset,
        dst.as_mut_ptr() as u64,
        dst.len() as u64,
        0,
    ))
}

fn call_flash_write(offset: u32, src: &[u8]) -> caplet_kernel::syscall::SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::FlashWrite.number(),
        offset,
        src.as_ptr() as u64,
        src.len() as u64,
        0,
    ))
}

fn call_flash_erase(offset: u32, sector_size: u32) -> caplet_kernel::syscall::SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::FlashErase.number(),
        offset,
        sector_size as u64,
        0,
        0,
    ))
}

fn trng_backend(buf: &mut [u8]) -> bool {
    for (index, byte) in buf.iter_mut().enumerate() {
        *byte = index as u8;
    }
    true
}

#[test]
fn crypto_and_flash_are_capability_gated() {
    clear_security_grants();

    // ── Crypto: no capability installed ────────────────────────────────
    let mut digest = [0u8; 32];
    let params = CryptoParams::new(3, 0, 0);
    let denied = call_crypto(4, &mut digest, b"abc", &params);
    assert!(!denied.success);
    assert_eq!(denied.error, CRYPTO_NO_TOKEN);

    install_crypto_capability().expect("install crypto capability");

    // SHA-256("abc") against the NIST vector.
    let ok = call_crypto(4, &mut digest, b"abc", &params);
    assert!(ok.success, "sha256: {ok:?}");
    assert_eq!(ok.value, 32);
    assert_eq!(
        digest,
        [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ]
    );

    // AES-128 ECB (FIPS-197 Appendix B).
    let key = [
        0x00u8, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f,
    ];
    let iv = [0u8; 16];
    let plaintext = [
        0x00u8, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff,
    ];
    let mut ciphertext = [0u8; 16];
    let params = CryptoParams::new(16, key.as_ptr() as usize, iv.as_ptr() as usize);
    let enc = call_crypto(0, &mut ciphertext, &plaintext, &params);
    assert!(enc.success, "ecb encrypt: {enc:?}");
    assert_eq!(
        ciphertext,
        [
            0x69, 0xc4, 0xe0, 0xd8, 0x6a, 0x7b, 0x04, 0x30, 0xd8, 0xcd, 0xb7, 0x80, 0x70, 0xb4,
            0xc5, 0x5a,
        ]
    );
    let mut roundtrip = [0u8; 16];
    let dec = call_crypto(1, &mut roundtrip, &ciphertext, &params);
    assert!(dec.success);
    assert_eq!(roundtrip, plaintext);

    // CBC round trip over three blocks.
    let original = [0x5au8; 48];
    let mut cbc = original;
    let enc = call_crypto(2, &mut cbc, &original, &params);
    assert!(enc.success, "cbc encrypt: {enc:?}");
    assert_ne!(cbc, original);
    let dec = call_crypto_in_place(3, &mut cbc, &params);
    assert!(dec.success, "cbc decrypt: {dec:?}");
    assert_eq!(cbc, original);

    // TRNG must not fabricate entropy when no backend is installed.
    set_trng_backend(None);
    let mut random = [0u8; 8];
    let unsupported = call_crypto(6, &mut random, &[], &CryptoParams::new(8, 0, 0));
    assert!(!unsupported.success);
    assert_eq!(unsupported.error, ERR_UNSUPPORTED);

    set_trng_backend(Some(trng_backend));
    let ok = call_crypto(6, &mut random, &[], &CryptoParams::new(8, 0, 0));
    assert!(ok.success);
    assert_eq!(random, [0, 1, 2, 3, 4, 5, 6, 7]);

    // ── Flash: no capability installed ─────────────────────────────────
    reset_flash_mirror();
    let mut buf = [0u8; 4];
    let denied = call_flash_read(FLASH_BASE, &mut buf);
    assert!(!denied.success);
    assert_eq!(denied.error, CRYPTO_NO_TOKEN);

    let end = FLASH_BASE + 3 * FLASH_SECTOR_SIZE - 1;
    install_flash_capability(FLASH_BASE, end, true, FLASH_SECTOR_SIZE)
        .expect("install flash capability");
    seed_flash_mirror(FLASH_BASE, b"RTOS").unwrap();

    let read = call_flash_read(FLASH_BASE, &mut buf);
    assert!(read.success, "flash read: {read:?}");
    assert_eq!(&buf, b"RTOS");

    // A read outside the grant is rejected even though it is inside the
    // simulated device.
    let mut outside = [0u8; 4];
    let rejected = call_flash_read(end + 1, &mut outside);
    assert!(!rejected.success);
    assert_eq!(rejected.error, ERR_OUT_OF_REGION);

    // NOR programming: 0xFF -> 0x00 is allowed, 0x00 -> 0xFF is not.
    let erased_region = FLASH_BASE + FLASH_SECTOR_SIZE;
    let write = call_flash_write(erased_region, &[0x00, 0x00]);
    assert!(write.success, "flash write: {write:?}");
    let mut verify = [0xFFu8; 2];
    assert!(call_flash_read(erased_region, &mut verify).success);
    assert_eq!(verify, [0x00, 0x00]);

    let illegal = call_flash_write(erased_region, &[0xFF, 0xFF]);
    assert!(!illegal.success);
    assert_eq!(illegal.error, ERR_NOT_ERASED);

    // Erase requires sector alignment and a matching sector size.
    let unaligned = call_flash_erase(erased_region + 16, FLASH_SECTOR_SIZE);
    assert!(!unaligned.success);
    assert_eq!(unaligned.error, ERR_BAD_ALIGNMENT);
    let wrong_size = call_flash_erase(erased_region, 512);
    assert!(!wrong_size.success);
    assert_eq!(wrong_size.error, ERR_BAD_ALIGNMENT);
    let erased = call_flash_erase(erased_region, FLASH_SECTOR_SIZE);
    assert!(erased.success, "erase: {erased:?}");
    assert!(call_flash_read(erased_region, &mut verify).success);
    assert_eq!(verify, [0xFF, 0xFF]);

    // The mirror is the actual backing store, not a no-op.
    let mut peeked = [0u8; 2];
    peek_flash_mirror(erased_region, &mut peeked).unwrap();
    assert_eq!(peeked, [0xFF, 0xFF]);

    // ── Read-only grant ────────────────────────────────────────────────
    clear_security_grants();
    install_flash_capability(FLASH_BASE, FLASH_BASE + 0xFF, false, FLASH_SECTOR_SIZE)
        .expect("install read-only flash capability");
    let mut bytes = [0u8; 2];
    assert!(call_flash_read(FLASH_BASE, &mut bytes).success);
    let protected = call_flash_write(FLASH_BASE, &[0x00, 0x00]);
    assert!(!protected.success);
    assert_eq!(protected.error, ERR_WRITE_PROTECTED);

    clear_security_grants();
}
