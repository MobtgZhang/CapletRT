//! Loader sandbox: high-level module-loading pipeline.
//!
//! `load_module` performs the full M3 load sequence:
//!
//! 1. Parse the container (ELF or `.capp`).
//! 2. Verify the signature according to the selected [`SignaturePolicy`].
//! 3. Allocate physical memory for the image.
//! 4. Apply relocations.
//! 5. Copy `.text + .data` to the allocated region.
//! 6. Create a [`LoadedModule`] struct the kernel can hand to a
//!    `Process` constructor.

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use caplet_cap::{CSpace, Permissions, SafetyBits};

use crate::allocator::ModulePool;
use crate::capp;
use crate::elf;
use crate::manifest::{ModuleFormat, ModuleManifest};
use crate::reloc;
use crate::verifier;

/// Errors emitted by `load_module`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderError {
    /// Unknown container magic.
    BadMagic,
    /// Underlying format parser rejected the input.
    ParseFailed,
    /// Signature verification failed.
    BadSignature,
    /// The selected policy requires a signature, but the module is unsigned.
    UnsignedModule,
    /// A signature is present but no crypto backend is compiled in.
    CryptoUnavailable,
    /// Out of memory in the module pool.
    OutOfMemory,
    /// Image too large for the pool's alignment.
    BadSize,
    /// Relocation went out of range.
    BadReloc,
    /// Entry point is outside the loaded image (for relocatable images).
    BadEntry,
    /// The loader does not hold a capability the module requires.
    MissingCapability,
}

/// Signature handling for [`load_module_with_policy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SignaturePolicy {
    /// Reject unsigned modules and require a working crypto backend. This is
    /// the default for [`load_module`].
    #[default]
    Enforce,
    /// Permit unsigned development modules. Signed modules are still fully
    /// verified and a present-but-unavailable crypto backend is still an
    /// error.
    AllowUnsigned,
}

/// A successfully loaded module.
pub struct LoadedModule {
    /// Manifest extracted from the container.
    pub manifest: ModuleManifest,
    /// Virtual address the image was loaded at.
    pub base_vaddr: u32,
    /// Physical address the image was loaded at.
    pub base_paddr: u32,
    /// CSpace pre-populated with the granted capabilities.
    pub cspace: CSpace,
    /// Image size in bytes.
    pub image_size: u32,
    /// Entry point virtual address after relocation.
    pub entry_vaddr: u32,
    /// Relocated `.text + .data` bytes, ready to be copied by the board.
    image: Vec<u8>,
}

impl LoadedModule {
    /// Borrow the relocated module image.
    pub fn image(&self) -> &[u8] {
        &self.image
    }

    /// Offset of the entry point from [`LoadedModule::base_vaddr`].
    pub fn entry_offset(&self) -> u32 {
        self.entry_vaddr.wrapping_sub(self.base_vaddr)
    }

    /// Copy the relocated image into a caller-owned mapping.
    pub fn copy_to(&self, dst: &mut [u8]) -> Result<(), LoaderError> {
        if dst.len() < self.image.len() {
            return Err(LoaderError::BadSize);
        }
        dst[..self.image.len()].copy_from_slice(&self.image);
        Ok(())
    }
}

/// Load a module from the given bytes. The module pool must be
/// configured before calling.
pub fn load_module(
    bytes: &[u8],
    pool: &mut ModulePool,
    loader_caps: &CSpace,
) -> Result<LoadedModule, LoaderError> {
    load_module_with_policy(bytes, pool, loader_caps, SignaturePolicy::Enforce)
}

/// Load a module with an explicit signature policy.
pub fn load_module_with_policy(
    bytes: &[u8],
    pool: &mut ModulePool,
    loader_caps: &CSpace,
    policy: SignaturePolicy,
) -> Result<LoadedModule, LoaderError> {
    if bytes.len() < 4 {
        return Err(LoaderError::BadMagic);
    }
    let (manifest, image, relocs) = {
        let magic: [u8; 4] = [bytes[0], bytes[1], bytes[2], bytes[3]];
        match &magic {
            m if *m == elf::ELF_MAGIC => {
                let parsed = elf::parse_elf(bytes).map_err(|_| LoaderError::ParseFailed)?;
                (parsed.manifest, parsed.image, parsed.relocs)
            }
            m if *m == capp::MAGIC => {
                let parsed = capp::parse_capp(bytes).map_err(|_| LoaderError::ParseFailed)?;
                (parsed.manifest, parsed.image, parsed.relocs)
            }
            _ => return Err(LoaderError::BadMagic),
        }
    };

    let must_verify = manifest.signed || policy == SignaturePolicy::Enforce;
    if must_verify && !manifest.signed {
        return Err(LoaderError::UnsignedModule);
    }
    if must_verify {
        match verifier::verify_signature(&manifest, &image) {
            Ok(()) => {}
            Err(verifier::SignatureError::Unavailable) => {
                return Err(LoaderError::CryptoUnavailable)
            }
            Err(_) => return Err(LoaderError::BadSignature),
        }
    }

    if !manifest.caps_consistent() {
        return Err(LoaderError::BadSignature);
    }

    // A module may only require capabilities that the loader itself holds,
    // with at least the requested permissions.  This prevents an attested
    // module from silently acquiring authority the loader never had.
    for required in &manifest.required_caps {
        let index = loader_caps
            .find_by_name(&required.tag)
            .ok_or(LoaderError::MissingCapability)?;
        let (perms, _) = loader_caps
            .slot_perms_safety(index)
            .ok_or(LoaderError::MissingCapability)?;
        if !Permissions(required.perms).is_subset(perms) {
            return Err(LoaderError::MissingCapability);
        }
    }

    // Allocate memory.
    let size = image.len() as u32;
    let (vaddr, paddr) = pool
        .alloc(size.max(0x10000))
        .ok_or(LoaderError::OutOfMemory)?;

    // Apply relocations in place.
    let mut img = image;
    if !relocs.is_empty() {
        match manifest.format {
            ModuleFormat::Capp if manifest.relocatable => {
                reloc::apply(&mut img, &relocs, 0, vaddr).map_err(|_| LoaderError::BadReloc)?;
            }
            ModuleFormat::Capp => return Err(LoaderError::BadReloc),
            ModuleFormat::Elf => {
                reloc::apply(&mut img, &relocs, manifest.text_base, vaddr)
                    .map_err(|_| LoaderError::BadReloc)?;
            }
        }
    }

    let entry_vaddr = match manifest.format {
        ModuleFormat::Capp if manifest.relocatable => {
            if manifest.entry_point as usize >= img.len() {
                return Err(LoaderError::BadEntry);
            }
            vaddr
                .checked_add(manifest.entry_point)
                .ok_or(LoaderError::BadEntry)?
        }
        ModuleFormat::Capp => manifest.entry_point,
        ModuleFormat::Elf => {
            if manifest.entry_point < manifest.text_base {
                return Err(LoaderError::BadEntry);
            }
            vaddr
                .checked_add(manifest.entry_point - manifest.text_base)
                .ok_or(LoaderError::BadEntry)?
        }
    };

    // The board's load_module shim copies `image` into the physical mapping
    // with its MMU/DMA helpers; keeping it here makes that step explicit and
    // allows host-side tests to execute the exact relocated bytes.

    let mut cspace = CSpace::new();
    for granted in &manifest.granted_caps {
        if let Ok(handle) = cspace.allocate(
            Permissions(granted.perms),
            SafetyBits::from_bits(granted.safety),
            None,
        ) {
            let _ = cspace.bind_name(handle.index(), &granted.tag);
        }
    }
    Ok(LoadedModule {
        manifest,
        base_vaddr: vaddr,
        base_paddr: paddr,
        cspace,
        image_size: size,
        entry_vaddr,
        image: img,
    })
}

/// Helper for the syscall layer: dump a manifest in a human-readable
/// format.
pub fn manifest_to_string(m: &ModuleManifest) -> String {
    let mut s = String::new();
    s.push_str(&format!("format: {:?}\n", m.format));
    s.push_str(&format!("name: {}\n", m.name_str()));
    s.push_str(&format!("version: {}\n", m.version));
    s.push_str(&format!("entry: 0x{:x}\n", m.entry_point));
    s.push_str(&format!(
        "text: 0x{:x} ({} bytes)\n",
        m.text_base, m.text_size
    ));
    s.push_str(&format!("data: {} bytes\n", m.data_size));
    s.push_str(&format!("required_caps: {}\n", m.required_caps.len()));
    s.push_str(&format!("granted_caps: {}\n", m.granted_caps.len()));
    s
}

/// Get a summary of the granted caps as a `Vec<(tag, perms)>`.
pub fn granted_caps_summary(m: &ModuleManifest) -> Vec<(String, u32)> {
    m.granted_caps
        .iter()
        .map(|c| (c.tag.clone(), c.perms))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::ELF_MAGIC;

    #[test]
    fn rejects_unknown_magic() {
        let bytes = [0u8; 32];
        let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
        let r = load_module_with_policy(
            &bytes,
            &mut pool,
            &CSpace::new(),
            SignaturePolicy::AllowUnsigned,
        );
        assert!(r.is_err());
    }

    #[test]
    fn parses_minimal_elf_via_loader() {
        // Build a minimal valid ELF.
        let mut elf = Vec::new();
        elf.extend_from_slice(&ELF_MAGIC);
        elf.push(1);
        elf.push(1);
        elf.push(1);
        elf.extend_from_slice(&[0u8; 9]);
        elf.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        elf.extend_from_slice(&40u16.to_le_bytes()); // EM_ARM
        elf.extend_from_slice(&1u32.to_le_bytes());
        elf.extend_from_slice(&0x1000u32.to_le_bytes());
        elf.extend_from_slice(&52u32.to_le_bytes()); // phoff = 52 (ehdr=52, 0 phdrs)
                                                     // Pad to 52 bytes
        elf.resize(52, 0);
        // No program headers, no image.
        let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
        // The minimal ELF without program headers is rejected by the
        // parser, so we expect an error here. That's intentional;
        // we only check that the loader does not panic.
        let r = load_module_with_policy(
            &elf,
            &mut pool,
            &CSpace::new(),
            SignaturePolicy::AllowUnsigned,
        );
        assert!(r.is_err());
    }

    #[test]
    fn parses_minimal_capp_via_loader() {
        let mut out = Vec::new();
        out.extend_from_slice(&capp::MAGIC);
        out.extend_from_slice(&capp::SUPPORTED_VERSION.to_le_bytes());
        out.extend_from_slice(&(capp::HEADER_SIZE as u16).to_le_bytes());
        out.extend_from_slice(&0x2000u32.to_le_bytes()); // entry
        out.extend_from_slice(&1024u32.to_le_bytes());
        out.push(8); // priority
        out.push(0); // flags (no signature required)
        out.extend_from_slice(&[0u8; 2]);
        // text @ HEADER_SIZE, 4 bytes
        let text_off = capp::HEADER_SIZE;
        out.extend_from_slice(&(text_off as u32).to_le_bytes());
        out.extend_from_slice(&4u32.to_le_bytes());
        // data @ text_off+4, 0 bytes
        let data_off = text_off + 4;
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        // relocs @ data_off, 0
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        // caps @ data_off, 0
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        // name @ data_off
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        // signature (64 bytes)
        out.extend_from_slice(&[0u8; capp::SIGNATURE_SIZE]);
        // text payload
        out.extend_from_slice(b"HELLO");
        // name
        out.extend_from_slice(b"m\x00\x00\x00");

        let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
        let loaded = load_module_with_policy(
            &out,
            &mut pool,
            &CSpace::new(),
            SignaturePolicy::AllowUnsigned,
        )
        .unwrap();
        assert_eq!(loaded.manifest.entry_point, 0x2000);
        assert_eq!(loaded.manifest.text_size, 4);
        assert_eq!(loaded.image_size, 4);
        assert_eq!(loaded.base_vaddr, 0x6000_0000);
    }

    #[test]
    fn grants_match_required() {
        let mut out = Vec::new();
        out.extend_from_slice(&capp::MAGIC);
        out.extend_from_slice(&capp::SUPPORTED_VERSION.to_le_bytes());
        out.extend_from_slice(&(capp::HEADER_SIZE as u16).to_le_bytes());
        out.extend_from_slice(&0x2000u32.to_le_bytes());
        out.extend_from_slice(&1024u32.to_le_bytes());
        out.push(8);
        out.push(0);
        out.extend_from_slice(&[0u8; 2]);
        let text_off = capp::HEADER_SIZE;
        out.extend_from_slice(&(text_off as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        let data_off = text_off;
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        let cap_off = data_off;
        out.extend_from_slice(&(cap_off as u32).to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        let name_off = cap_off + 32;
        out.extend_from_slice(&(name_off as u32).to_le_bytes());
        out.extend_from_slice(&[0u8; capp::SIGNATURE_SIZE]);
        // cap descriptor: 28-byte tag "log" + 4-byte perms
        let mut cap = [0u8; 32];
        cap[..3].copy_from_slice(b"log");
        cap[28..32].copy_from_slice(&0xFFu32.to_le_bytes());
        out.extend_from_slice(&cap);
        // name
        out.extend_from_slice(b"m\x00\x00\x00");

        let mut loader_caps = CSpace::new();
        let handle = loader_caps
            .allocate(Permissions(0xFF), SafetyBits::default(), None)
            .unwrap();
        loader_caps.bind_name(handle.index(), "log").unwrap();

        let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
        let loaded = load_module_with_policy(
            &out,
            &mut pool,
            &loader_caps,
            SignaturePolicy::AllowUnsigned,
        )
        .unwrap();
        assert_eq!(loaded.manifest.granted_caps.len(), 1);
        assert_eq!(loaded.manifest.granted_caps[0].tag, "log");
        assert!(loaded.cspace.find_by_name("log").is_some());
    }
}
