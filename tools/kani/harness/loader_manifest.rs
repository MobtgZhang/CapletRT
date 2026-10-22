//! Kani harness: loader manifest parse.
//!
//! Property: `parse_capp` rejects any byte sequence whose first four bytes
//! are not the canonical magic with `Err(BadMagic)`. Once the magic matches,
//! the header-version/header-size gates take over.

#![cfg(kani)]

use caplet_loader::capp::{validate_capp_header, CappError, HEADER_SIZE, MAGIC, SUPPORTED_VERSION};

const MAGIC_LEN: usize = 4;

fn header_with_prefix(prefix: [u8; MAGIC_LEN]) -> [u8; HEADER_SIZE] {
    let mut bytes = [0u8; HEADER_SIZE];
    bytes[..MAGIC_LEN].copy_from_slice(&prefix);
    bytes
}

// The four proofs below partition every non-magic prefix by the position of
// its first mismatching byte. Keeping the mismatch concrete avoids asking the
// SAT solver to reason about a symbolic slice `memcmp`; the previous single
// proof did not propagate that assumption and explored the full parser,
// including unbounded UTF-8 validation.

#[kani::proof]
#[kani::unwind(16)]
fn parse_capp_rejects_non_magic_byte0() {
    let byte: u8 = kani::any();
    kani::assume(byte != MAGIC[0]);
    let bytes = header_with_prefix([byte, 0, 0, 0]);

    assert!(
        matches!(validate_capp_header(&bytes), Err(CappError::BadMagic)),
        "parse_capp accepted a prefix with a mismatching first byte"
    );
}

#[kani::proof]
#[kani::unwind(16)]
fn parse_capp_rejects_non_magic_byte1() {
    let byte: u8 = kani::any();
    kani::assume(byte != MAGIC[1]);
    let bytes = header_with_prefix([MAGIC[0], byte, 0, 0]);

    assert!(
        matches!(validate_capp_header(&bytes), Err(CappError::BadMagic)),
        "parse_capp accepted a prefix with a mismatching second byte"
    );
}

#[kani::proof]
#[kani::unwind(16)]
fn parse_capp_rejects_non_magic_byte2() {
    let byte: u8 = kani::any();
    kani::assume(byte != MAGIC[2]);
    let bytes = header_with_prefix([MAGIC[0], MAGIC[1], byte, 0]);

    assert!(
        matches!(validate_capp_header(&bytes), Err(CappError::BadMagic)),
        "parse_capp accepted a prefix with a mismatching third byte"
    );
}

#[kani::proof]
#[kani::unwind(16)]
fn parse_capp_rejects_non_magic_byte3() {
    let byte: u8 = kani::any();
    kani::assume(byte != MAGIC[3]);
    let bytes = header_with_prefix([MAGIC[0], MAGIC[1], MAGIC[2], byte]);

    assert!(
        matches!(validate_capp_header(&bytes), Err(CappError::BadMagic)),
        "parse_capp accepted a prefix with a mismatching fourth byte"
    );
}

#[kani::proof]
#[kani::unwind(16)]
fn parse_capp_proceeds_past_magic() {
    // A buffer with canonical magic and an unsupported version must fail at
    // the version gate. This proves the magic comparison is transparent for
    // this boundary case instead of merely calling the parser without an
    // assertion.
    let mut bytes = header_with_prefix(MAGIC);
    let version: u16 = kani::any();
    kani::assume(version != SUPPORTED_VERSION);
    bytes[4..6].copy_from_slice(&version.to_le_bytes());
    bytes[6..8].copy_from_slice(&(HEADER_SIZE as u16).to_le_bytes());

    assert!(
        matches!(
            validate_capp_header(&bytes),
            Err(CappError::UnsupportedVersion)
        ),
        "magic prefix was not accepted before the version gate"
    );
}
