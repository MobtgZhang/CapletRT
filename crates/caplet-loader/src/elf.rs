//! ELF subset parser.
//!
//! We accept only:
//! - 32-bit, little-endian, ARM ELF (e_machine == EM_ARM, ei_class == ELFCLASS32)
//! - ET_EXEC executables (no shared libs, no PIE)
//! - PT_LOAD program headers; everything else is ignored or rejected
//! - R_ARM_RELATIVE relocations (the only relocation we implement)
//!
//! Anything outside the whitelist is rejected with `ElfError`. The
//! intention is to support the common "compile with
//! `--target thumbv7em-none-eabihf` and embed the resulting `.elf`"
//! workflow while keeping the loader code small and auditable.

extern crate alloc;
use alloc::vec::Vec;

use crate::manifest::{ModuleFormat, ModuleManifest};

/// `e_machine` value for ARM.
pub const EM_ARM: u16 = 40;
/// `e_type` value for ET_EXEC.
pub const ET_EXEC: u16 = 2;
/// `p_type` value for PT_LOAD.
pub const PT_LOAD: u32 = 1;
/// `p_type` value for PT_INTERP. We reject any module that has one.
pub const PT_INTERP: u32 = 3;
/// `p_type` value for PT_DYNAMIC. Also rejected.
pub const PT_DYNAMIC: u32 = 2;
/// ELF magic.
pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
/// `R_ARM_RELATIVE` (0x23).
pub const R_ARM_RELATIVE: u32 = 23;

/// ELF header (32-bit).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Elf32Ehdr {
    /// Magic + class + data + version + OSABI + pad[8]
    pub ident: [u8; 16],
    /// Object file type.
    pub e_type: u16,
    /// Target architecture.
    pub e_machine: u16,
    /// Object file version.
    pub e_version: u32,
    /// Entry point virtual address.
    pub e_entry: u32,
    /// Program header table file offset.
    pub e_phoff: u32,
    /// Section header table file offset.
    pub e_shoff: u32,
    /// Processor-specific flags.
    pub e_flags: u32,
    /// ELF header size in bytes.
    pub e_ehsize: u16,
    /// Program header table entry size.
    pub e_phentsize: u16,
    /// Number of program header entries.
    pub e_phnum: u16,
    /// Section header table entry size.
    pub e_shentsize: u16,
    /// Number of section header entries.
    pub e_shnum: u16,
    /// Section header table index of the section name string table.
    pub e_shstrndx: u16,
}

/// Program header (32-bit).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Elf32Phdr {
    /// Segment type.
    pub p_type: u32,
    /// Segment offset in file.
    pub p_offset: u32,
    /// Segment virtual address.
    pub p_vaddr: u32,
    /// Segment physical address (unused on Linux/embedded; we use
    /// `p_vaddr` for the kernel's purpose).
    pub p_paddr: u32,
    /// Segment size in file.
    pub p_filesz: u32,
    /// Segment size in memory.
    pub p_memsz: u32,
    /// Segment flags.
    pub p_flags: u32,
    /// Segment alignment.
    pub p_align: u32,
}

/// Parsed ELF output.
#[derive(Debug)]
pub struct ParsedElf {
    /// Manifest extracted from the ELF.
    pub manifest: ModuleManifest,
    /// Concatenated `.text + .data` blob (each PT_LOAD segment
    /// concatenated).
    pub image: Vec<u8>,
    /// Base virtual address of the image (the lowest PT_LOAD
    /// `p_vaddr`).
    pub base_vaddr: u32,
    /// Relocations to apply. Each entry is a (vaddr, addend) pair.
    pub relocs: Vec<(u32, u32)>,
}

/// Errors emitted by the ELF parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    /// Input is too short.
    TooShort,
    /// First four bytes are not the ELF magic.
    BadMagic,
    /// Not a 32-bit ELF.
    Not32Bit,
    /// Not little-endian.
    NotLittleEndian,
    /// Architecture is not ARM.
    NotArm,
    /// Type is not ET_EXEC.
    NotExecutable,
    /// PT_INTERP or PT_DYNAMIC present.
    DynamicLinking,
    /// Program header offset is out of range.
    PhdrOutOfRange,
    /// PT_LOAD file range is out of range.
    LoadOutOfRange,
    /// ELF header is truncated.
    Truncated,
}

/// Parse the given bytes as an ELF executable.
///
/// On success, returns a [`ParsedElf`]. On failure, returns an
/// [`ElfError`].
pub fn parse_elf(bytes: &[u8]) -> Result<ParsedElf, ElfError> {
    if bytes.len() < core::mem::size_of::<Elf32Ehdr>() {
        return Err(ElfError::TooShort);
    }
    // Read the header manually so we don't require a copy or a
    // pointer-alignment trick.
    let mut ident = [0u8; 16];
    ident.copy_from_slice(&bytes[..16]);
    if ident[..4] != ELF_MAGIC {
        return Err(ElfError::BadMagic);
    }
    if ident[4] != 1 {
        return Err(ElfError::Not32Bit);
    }
    if ident[5] != 1 {
        return Err(ElfError::NotLittleEndian);
    }
    // Read remaining header fields (offsets match `Elf32Ehdr`).
    let read_u16 = |off: usize| -> u16 {
        u16::from_le_bytes([bytes[off], bytes[off + 1]])
    };
    let read_u32 = |off: usize| -> u32 {
        u16::from_le_bytes([bytes[off], bytes[off + 1]]) as u32;
        u32::from_le_bytes([
            bytes[off],
            bytes[off + 1],
            bytes[off + 2],
            bytes[off + 3],
        ])
    };
    let e_type = read_u16(16);
    let e_machine = read_u16(18);
    let _e_version = read_u32(20);
    let e_entry = read_u32(24);
    let e_phoff = read_u32(28);
    let _e_shoff = read_u32(32);
    let _e_flags = read_u32(36);
    let _e_ehsize = read_u16(40);
    let _e_phentsize = read_u16(42);
    let e_phnum = read_u16(44) as usize;
    let _e_shentsize = read_u16(46);
    let _e_shnum = read_u16(48);
    let _e_shstrndx = read_u16(50);

    if e_type != ET_EXEC {
        return Err(ElfError::NotExecutable);
    }
    if e_machine != EM_ARM {
        return Err(ElfError::NotArm);
    }
    if e_phnum == 0 || e_phoff == 0 {
        return Err(ElfError::PhdrOutOfRange);
    }
    if e_phoff as usize + e_phnum * core::mem::size_of::<Elf32Phdr>() > bytes.len() {
        return Err(ElfError::Truncated);
    }

    // Walk the program headers. We collect PT_LOAD segments and
    // reject PT_INTERP / PT_DYNAMIC.
    let mut segments: Vec<(Elf32Phdr, &[u8])> = Vec::new();
    for i in 0..e_phnum {
        let off = e_phoff as usize + i * core::mem::size_of::<Elf32Phdr>();
        let ph = Elf32Phdr {
            p_type: read_u32(off),
            p_offset: read_u32(off + 4),
            p_vaddr: read_u32(off + 8),
            p_paddr: read_u32(off + 12),
            p_filesz: read_u32(off + 16),
            p_memsz: read_u32(off + 20),
            p_flags: read_u32(off + 24),
            p_align: read_u32(off + 28),
        };
        match ph.p_type {
            PT_LOAD => {
                let start = ph.p_offset as usize;
                let end = start + ph.p_filesz as usize;
                if end > bytes.len() {
                    return Err(ElfError::LoadOutOfRange);
                }
                segments.push((ph, &bytes[start..end]));
            }
            PT_INTERP | PT_DYNAMIC => return Err(ElfError::DynamicLinking),
            _ => {}
        }
    }
    // Concatenate segments into a single image.
    let mut image = Vec::new();
    let mut base_vaddr: u32 = u32::MAX;
    for (ph, slice) in &segments {
        if ph.p_vaddr < base_vaddr {
            base_vaddr = ph.p_vaddr;
        }
        image.extend_from_slice(slice);
    }
    if base_vaddr == u32::MAX {
        base_vaddr = 0;
    }

    let mut manifest = ModuleManifest::new(ModuleFormat::Elf);
    manifest.entry_point = e_entry;
    manifest.text_base = base_vaddr;
    manifest.text_size = image.len() as u32;

    // We have no relocation table here (the R_ARM_RELATIVE entries
    // are inside the `.rel.dyn` section, which we don't parse for
    // v1); leave `relocs` empty. The whole-image `__caplet_relocate`
    // hook can be added later.

    Ok(ParsedElf {
        manifest,
        image,
        base_vaddr,
        relocs: Vec::new(),
    })
}

#[allow(dead_code)]
const _ELF_MAGIC_CHECK: [u8; 4] = ELF_MAGIC;

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_elf(bytes: &[u8]) -> Vec<u8> {
        // Build a 32-bit ARM ELF executable with one PT_LOAD
        // segment and no program headers beyond that.
        let mut out = Vec::new();
        out.extend_from_slice(&ELF_MAGIC);
        out.push(1); // 32-bit
        out.push(1); // LE
        out.push(1); // ELF version
        out.extend_from_slice(&[0u8; 9]);
        out.extend_from_slice(&ET_EXEC.to_le_bytes());
        out.extend_from_slice(&EM_ARM.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes()); // e_version
        out.extend_from_slice(&0x1000u32.to_le_bytes()); // e_entry
        out.extend_from_slice(&52u32.to_le_bytes()); // e_phoff (size of ehdr)
        out.extend_from_slice(&[0u8; 8]); // e_shoff + e_flags
        out.extend_from_slice(&52u16.to_le_bytes()); // e_ehsize (at offset 40)
        out.extend_from_slice(&32u16.to_le_bytes()); // e_phentsize (at offset 42)
        out.extend_from_slice(&1u16.to_le_bytes()); // e_phnum (at offset 44)
        out.extend_from_slice(&[0u8; 6]); // e_shentsize + e_shnum + e_shstrndx
        // Program header
        out.extend_from_slice(&PT_LOAD.to_le_bytes());
        out.extend_from_slice(&84u32.to_le_bytes()); // p_offset (after ehdr+phdr)
        out.extend_from_slice(&0x1000u32.to_le_bytes()); // p_vaddr
        out.extend_from_slice(&[0u8; 4]); // p_paddr
        let payload = bytes;
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // p_filesz
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // p_memsz
        out.extend_from_slice(&[0u8; 4]); // p_flags
        out.extend_from_slice(&[0u8; 4]); // p_align
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn rejects_bad_magic() {
        let bytes = [0u8; 64];
        assert!(parse_elf(&bytes).is_err());
    }

    #[test]
    fn parses_minimal_elf() {
        let elf = minimal_elf(b"hello");
        let parsed = parse_elf(&elf).unwrap();
        assert_eq!(parsed.manifest.entry_point, 0x1000);
        assert_eq!(parsed.manifest.text_size, 5);
        assert_eq!(parsed.image.as_slice(), b"hello");
    }
}