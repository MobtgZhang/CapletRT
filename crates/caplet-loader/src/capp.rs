//! Custom `.capp` container format.
//!
//! The binary layout is:
//!
//! ```text
//! header:
//!   magic (4B) "CAPP"
//!   version (2B)
//!   hdr_size (2B) = sizeof(header)
//!   entry_point (4B)
//!   stack_size (4B)
//!   priority (1B)
//!   flags (1B)
//!   _pad (2B)
//!   text_off (4B)
//!   text_size (4B)
//!   data_off (4B)
//!   data_size (4B)
//!   reloc_off (4B)
//!   reloc_count (4B)
//!   cap_manifest_off (4B)
//!   cap_manifest_count (4B)
//!   name_off (4B)
//!   signature (64B)
//! ---- sections follow ----
//!   text:   payload
//!   data:   payload
//!   relocs: each 8B (vaddr u32, addend u32)
//!   caps:   each 32B cap descriptor
//!   name:   UTF-8 module name, null-padded to 4-byte boundary
//! ```
//!
//! The header is exactly 80 bytes long. This is verified at parse
//! time so a malformed container is rejected.

extern crate alloc;
#[cfg(test)]
extern crate std;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::manifest::{CapDescriptor, ModuleFormat, ModuleManifest};

/// Header size in bytes. All offsets in the layout are relative to
/// the start of the container.
///
/// Layout (offsets in bytes):
///
/// ```text
/// 0..4     magic
/// 4..6     version
/// 6..8     hdr_size
/// 8..12    entry_point
/// 12..16   stack_size
/// 16       priority
/// 17       flags
/// 18..20   _pad
/// 20..24   text_off
/// 24..28   text_size
/// 28..32   data_off
/// 32..36   data_size
/// 36..40   reloc_off
/// 40..44   reloc_count
/// 44..48   cap_manifest_off
/// 48..52   cap_manifest_count
/// 52..56   name_off
/// 56..120  signature (64 bytes)
/// ```
pub const HEADER_SIZE: usize = 120;

/// Signature length in bytes (Ed25519).
pub const SIGNATURE_SIZE: usize = 64;

/// Magic bytes.
pub const MAGIC: [u8; 4] = [b'C', b'A', b'P', b'P'];

/// Container version supported by this parser.
pub const SUPPORTED_VERSION: u16 = 1;

/// Flags for the `.capp` header.
pub const FLAG_SIGNED: u8 = 1 << 0;
/// Flag indicating the container is relocatable (image is
/// position-independent; the loader must apply `R_ARM_RELATIVE`
/// relocations to its base address).
pub const FLAG_RELOCATABLE: u8 = 1 << 1;

/// Parsed `.capp` output.
#[derive(Debug)]
pub struct ParsedCapp {
    /// Manifest extracted from the container.
    pub manifest: ModuleManifest,
    /// Concatenated `.text + .data` image.
    pub image: Vec<u8>,
    /// Relocations to apply.
    pub relocs: Vec<(u32, u32)>,
}

/// Errors emitted by the `.capp` parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CappError {
    /// Input is shorter than the header.
    TooShort,
    /// Magic mismatch.
    BadMagic,
    /// Header size field does not match.
    BadHeaderSize,
    /// Container version not supported.
    UnsupportedVersion,
    /// Section offset is out of range.
    SectionOutOfRange,
    /// Section size would overflow the container.
    BadSectionSize,
    /// Header claims signed but no key id set.
    MissingKeyId,
}

/// Parse the given bytes as a `.capp` container.
pub fn parse_capp(bytes: &[u8]) -> Result<ParsedCapp, CappError> {
    if bytes.len() < HEADER_SIZE {
        return Err(CappError::TooShort);
    }
    if bytes[..4] != MAGIC {
        return Err(CappError::BadMagic);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != SUPPORTED_VERSION {
        return Err(CappError::UnsupportedVersion);
    }
    let hdr_size = u16::from_le_bytes([bytes[6], bytes[7]]);
    if hdr_size as usize != HEADER_SIZE {
        return Err(CappError::BadHeaderSize);
    }
    let entry_point = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let stack_size = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    let priority = bytes[16];
    let flags = bytes[17];
    let _pad = u16::from_le_bytes([bytes[18], bytes[19]]);
    let text_off = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as usize;
    let text_size = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]) as usize;
    let data_off = u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]) as usize;
    let data_size = u32::from_le_bytes([bytes[32], bytes[33], bytes[34], bytes[35]]) as usize;
    let reloc_off = u32::from_le_bytes([bytes[36], bytes[37], bytes[38], bytes[39]]) as usize;
    let reloc_count = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]) as usize;
    let cap_off = u32::from_le_bytes([bytes[44], bytes[45], bytes[46], bytes[47]]) as usize;
    let cap_count = u32::from_le_bytes([bytes[48], bytes[49], bytes[50], bytes[51]]) as usize;
    let name_off = u32::from_le_bytes([bytes[52], bytes[53], bytes[54], bytes[55]]) as usize;
    let mut signature = [0u8; SIGNATURE_SIZE];
    signature.copy_from_slice(&bytes[56..56 + SIGNATURE_SIZE]);

    let mut manifest = ModuleManifest::new(ModuleFormat::Capp);
    manifest.entry_point = entry_point;
    manifest.stack_size = stack_size;
    manifest.priority = priority;
    manifest.text_size = text_size as u32;
    manifest.data_size = data_size as u32;
    manifest.signature = signature;
    if (flags & FLAG_SIGNED) != 0 && manifest.public_key_id == 0 {
        return Err(CappError::MissingKeyId);
    }

    // Name section.
    #[cfg(test)]
    std::eprintln!("name_off={} bytes_len={} bytes_after_name_off={:?}", name_off, bytes.len(), &bytes[name_off..bytes.len().min(name_off + 16)]);
    if name_off < bytes.len() {
        let end = find_null_or_end(&bytes[name_off..]);
        let name_str = match core::str::from_utf8(&bytes[name_off..name_off + end]) {
            Ok(s) => s,
            Err(_) => "<non-utf8>",
        };
        #[cfg(test)]
        std::eprintln!("name bytes={:?}", &bytes[name_off..name_off + end]);
        manifest.set_name(name_str);
    }

    // Cap manifest.
    if cap_off.checked_add(cap_count * 32).map(|e| e > bytes.len()).unwrap_or(true) {
        return Err(CappError::SectionOutOfRange);
    }
    for i in 0..cap_count {
        let off = cap_off + i * 32;
        // Each cap descriptor is 32 bytes:
        //  - tag: 28 bytes (null-padded UTF-8)
        //  - perms: 4 bytes
        let mut tag_bytes = [0u8; 28];
        tag_bytes.copy_from_slice(&bytes[off..off + 28]);
        let tag_str = match core::str::from_utf8(&tag_bytes) {
            Ok(s) => s
                .split('\0')
                .next()
                .unwrap_or("<bad>")
                .to_string(),
            Err(_) => "<bad>".to_string(),
        };
        let perms = u32::from_le_bytes([
            bytes[off + 28],
            bytes[off + 29],
            bytes[off + 30],
            bytes[off + 31],
        ]);
        manifest.granted_caps.push(CapDescriptor::new(tag_str, perms, 0));
    }
    // For M3 we treat required_caps == granted_caps; a finer model
    // would let the loader grant a superset.
    manifest.required_caps = manifest.granted_caps.clone();

    // Relocations.
    let reloc_bytes = reloc_count * 8;
    if reloc_off.checked_add(reloc_bytes).map(|e| e > bytes.len()).unwrap_or(true) {
        return Err(CappError::SectionOutOfRange);
    }
    let mut relocs = Vec::with_capacity(reloc_count);
    for i in 0..reloc_count {
        let off = reloc_off + i * 8;
        let vaddr = u32::from_le_bytes([
            bytes[off],
            bytes[off + 1],
            bytes[off + 2],
            bytes[off + 3],
        ]);
        let addend = u32::from_le_bytes([
            bytes[off + 4],
            bytes[off + 5],
            bytes[off + 6],
            bytes[off + 7],
        ]);
        relocs.push((vaddr, addend));
    }

    // Image = text + data.
    if text_off.checked_add(text_size).map(|e| e > bytes.len()).unwrap_or(true) {
        return Err(CappError::BadSectionSize);
    }
    if data_off.checked_add(data_size).map(|e| e > bytes.len()).unwrap_or(true) {
        return Err(CappError::BadSectionSize);
    }
    let mut image = Vec::with_capacity(text_size + data_size);
    image.extend_from_slice(&bytes[text_off..text_off + text_size]);
    image.extend_from_slice(&bytes[data_off..data_off + data_size]);
    manifest.text_base = text_off as u32;

    Ok(ParsedCapp { manifest, image, relocs })
}

fn find_null_or_end(bytes: &[u8]) -> usize {
    bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_capp() -> Vec<u8> {
        // Header (120 bytes) + name + 1 cap descriptor + 0 relocs.
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&SUPPORTED_VERSION.to_le_bytes());
        out.extend_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
        out.extend_from_slice(&0x2000u32.to_le_bytes()); // entry
        out.extend_from_slice(&1024u32.to_le_bytes()); // stack
        out.push(8); // priority
        out.push(0); // flags (no signature required for the test)
        out.extend_from_slice(&[0u8; 2]); // pad
        // text @ HEADER_SIZE, 4 bytes
        let text_off = HEADER_SIZE;
        out.extend_from_slice(&(text_off as u32).to_le_bytes());
        out.extend_from_slice(&4u32.to_le_bytes());
        // data @ text_off+4, 0 bytes
        let data_off = text_off + 4;
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        // relocs @ data_off, 0
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        // caps @ data_off, 1
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        // name @ data_off + 32, 5 bytes
        let name_off = data_off + 32;
        out.extend_from_slice(&(name_off as u32).to_le_bytes());
        // signature (64 bytes zero) at bytes[56..120]
        out.extend_from_slice(&[0u8; SIGNATURE_SIZE]);
        // text payload (4 bytes to match text_size)
        out.extend_from_slice(b"HEL\x00");
        // cap descriptor: 28-byte tag + 4-byte perms = 32 bytes
        out.extend_from_slice(b"hello\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00");
        out.extend_from_slice(&0xFFu32.to_le_bytes());
        // name
        out.extend_from_slice(b"hello\x00");
        out
    }

    #[test]
    fn parses_minimal_capp() {
        let bytes = build_minimal_capp();
        let parsed = parse_capp(&bytes).unwrap();
        assert_eq!(parsed.manifest.entry_point, 0x2000);
        assert_eq!(parsed.manifest.text_size, 4);
        assert_eq!(parsed.manifest.name_str(), "hello");
        assert_eq!(parsed.manifest.granted_caps.len(), 1);
        assert_eq!(parsed.image.len(), 4);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = build_minimal_capp();
        bytes[0] = b'X';
        assert!(parse_capp(&bytes).is_err());
    }
}