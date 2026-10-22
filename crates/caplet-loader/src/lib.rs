//! CapletRT dynamic module loader (M3).
//!
//! The loader accepts two container formats:
//!
//! - **ELF subset**: 32-bit ARM ELF, ET_EXEC only, PT_LOAD segments
//!   only, R_ARM_RELATIVE relocations only. The goal is to be
//!   compatible with the existing developer workflow (compile with
//!   `cargo build --target thumbv7em-none-eabihf`) without bringing
//!   in a full dynamic linker.
//!
//! - **`.capp`**: a compact custom container with an explicit cap
//!   manifest. This is the production-deployment format because it
//!   is smaller, has no implicit relocations, and forces the loader
//!   to enumerate every capability the module needs.
//!
//! Both formats share the [`manifest::ModuleManifest`] struct and
//! the [`verifier`] path. The format-specific parser is selected by
//! inspecting the first four bytes of the input (`0x7f 'E' 'L' 'F'`
//! vs `"CAPP"`).
//!
//! docv1.md §17.1 mandates that the dynamic mode co-exists with the
//! static mode: the M3 demo (`boards/hello-dynamic-loader`) loads a
//! `.capp` at boot, while `boards/industrial-demo` continues to
//! link statically.

#![deny(missing_docs)]
#![no_std]

extern crate alloc;

pub mod allocator;
pub mod capp;
pub mod elf;
pub mod manifest;
pub mod metrics;
pub mod reloc;
pub mod sandbox;
pub mod verifier;

pub use capp::parse_capp;
pub use elf::parse_elf;
pub use manifest::{CapDescriptor, ModuleFormat, ModuleManifest};
pub use sandbox::{
    load_module, load_module_with_policy, LoadedModule, LoaderError, SignaturePolicy,
};
pub use verifier::{
    clear_trust_store, current_trust_store, install_trust_store, signing_message, verify_signature,
    verify_signature_with_store, SignatureError, SigningKey, TrustStore,
};

/// Identifier of a loaded module. Distinct from a process id: a
/// module may contain several processes, or one process may be
/// composed of several modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId(pub u32);

/// Entry point function signature for a loaded module.
pub type ModuleEntry = extern "C" fn(u32) -> u32;
