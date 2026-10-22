//! Module manifest.
//!
//! Shared by the ELF and `.capp` parsers. Every module carries a
//! manifest, which is the kernel's source of truth for what
//! capabilities the module needs and what it is granted.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

/// Format of a loaded container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleFormat {
    /// 32-bit ARM ELF executable (sub-Linux, no dynamic linking).
    Elf,
    /// Custom CapletRT container.
    Capp,
}

/// A capability referenced by the module's manifest. `tag` is the
/// human-readable name (e.g. `"uart0"`, `"log"`); `perms` and
/// `safety` are the bits granted to the loader.
#[derive(Debug, Clone)]
pub struct CapDescriptor {
    /// Symbolic name.
    pub tag: String,
    /// Permissions bitmask.
    pub perms: u32,
    /// Safety bits.
    pub safety: u8,
}

impl CapDescriptor {
    /// Construct from a string tag and permission/safety values.
    pub fn new(tag: impl Into<String>, perms: u32, safety: u8) -> Self {
        Self {
            tag: tag.into(),
            perms,
            safety,
        }
    }
}

/// Common manifest extracted from either container format.
#[derive(Debug, Clone)]
pub struct ModuleManifest {
    /// Container format.
    pub format: ModuleFormat,
    /// Module name (up to 32 bytes; zero-padded).
    pub name: [u8; 32],
    /// Length of the populated portion of `name`.
    pub name_len: usize,
    /// Manifest version. Bumped when on-disk layout changes.
    pub version: u16,
    /// Capabilities the module *requires* (must be present in the
    /// loader's CSpace for the load to succeed).
    pub required_caps: Vec<CapDescriptor>,
    /// Capabilities the module *requests* to receive. The kernel
    /// may grant fewer (down to `required_caps`) but never more.
    pub granted_caps: Vec<CapDescriptor>,
    /// Address of the entry point.
    pub entry_point: u32,
    /// Stack size in bytes. The loader provides the stack.
    pub stack_size: u32,
    /// Static priority hint. The scheduler may or may not honour it.
    pub priority: u8,
    /// Virtual address of `.text` start (loader fills in for ELF).
    pub text_base: u32,
    /// `.text` size in bytes.
    pub text_size: u32,
    /// `.data` size in bytes.
    pub data_size: u32,
    /// Identifier of the public key needed to verify the signature.
    pub public_key_id: u32,
    /// Whether the container carries an Ed25519 signature that must be
    /// verified before loading.
    pub signed: bool,
    /// Whether `.capp` entry point and relocation addresses are relative to
    /// the image base.
    pub relocatable: bool,
    /// Ed25519 signature over the rest of the container.
    pub signature: [u8; 64],
}

impl ModuleManifest {
    /// Construct a default manifest.
    pub fn new(format: ModuleFormat) -> Self {
        Self {
            format,
            name: [0u8; 32],
            name_len: 0,
            version: 1,
            required_caps: Vec::new(),
            granted_caps: Vec::new(),
            entry_point: 0,
            stack_size: 4096,
            priority: 16,
            text_base: 0,
            text_size: 0,
            data_size: 0,
            public_key_id: 0,
            signed: false,
            relocatable: false,
            signature: [0u8; 64],
        }
    }

    /// Populate the `name` field, truncating to 32 bytes.
    pub fn set_name(&mut self, n: &str) {
        let bytes = n.as_bytes();
        let n = bytes.len().min(self.name.len());
        self.name[..n].copy_from_slice(&bytes[..n]);
        self.name_len = n;
    }

    /// Read the name as a UTF-8 string (lossy when the manifest was
    /// constructed from non-UTF8 bytes).
    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("<non-utf8>")
    }

    /// Returns `true` if `required_caps` is a subset of
    /// `granted_caps` (set-theoretic containment by tag).
    pub fn caps_consistent(&self) -> bool {
        for req in &self.required_caps {
            if !self.granted_caps.iter().any(|g| g.tag == req.tag) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_truncates_at_32() {
        let mut m = ModuleManifest::new(ModuleFormat::Capp);
        m.set_name(&"a".repeat(40));
        assert_eq!(m.name_len, 32);
    }

    #[test]
    fn caps_consistency() {
        let mut m = ModuleManifest::new(ModuleFormat::Elf);
        m.required_caps.push(CapDescriptor::new("uart0", 0xFF, 0));
        m.granted_caps.push(CapDescriptor::new("uart0", 0xFF, 0));
        assert!(m.caps_consistent());

        m.required_caps.push(CapDescriptor::new("log", 0x01, 0));
        // log is required but not granted
        assert!(!m.caps_consistent());

        m.granted_caps.push(CapDescriptor::new("log", 0x01, 0));
        assert!(m.caps_consistent());
    }
}
