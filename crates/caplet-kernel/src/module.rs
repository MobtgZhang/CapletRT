//! Kernel-owned dynamic module registry.
//!
//! The loader crate is deliberately independent of the kernel. This module
//! owns the persistent module pool, the loaded-module table, and the board
//! hooks that map an image into executable memory and invoke its entry point.

extern crate alloc;

use alloc::collections::BTreeMap;
use caplet_cap::CSpace;
use caplet_loader::allocator::ModulePool;
use caplet_loader::sandbox::{load_module, LoadedModule, LoaderError, SignaturePolicy};

/// Copy a relocated module image into executable memory.
///
/// Returns `false` if the caller cannot map the image. The kernel never
/// writes to a caller-supplied address itself; a board (or a host test)
/// must install this hook.
pub type ModuleMapFn = fn(base: u32, image: &[u8]) -> bool;

/// Invoke a module entry point.
///
/// # Safety
///
/// `entry` must point to executable memory containing the module image
/// previously mapped by the corresponding [`ModuleMapFn`].
pub type ModuleEntryFn = unsafe extern "C" fn(entry: u32, arg: u32) -> u32;

/// Board-provided module runtime hooks.
#[derive(Debug, Clone, Copy)]
pub struct ModuleHooks {
    /// Copy the relocated image to its load address.
    pub map: ModuleMapFn,
    /// Enter mapped module code.
    pub execute: ModuleEntryFn,
}

/// Errors returned by [`ModuleRegistry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleError {
    /// No runtime hooks have been installed.
    NoHooks,
    /// The requested module id is unknown.
    NotFound,
    /// The module id was already registered.
    AlreadyRegistered,
    /// The module has not been registered for execution.
    NotRegistered,
    /// The module pool was already configured with a different range.
    PoolConflict,
    /// The pool range is invalid.
    BadPool,
    /// The image could not be mapped.
    MapFailed,
    /// Parsing, verification, relocation, or allocation failed.
    Loader(LoaderError),
}

impl From<LoaderError> for ModuleError {
    fn from(error: LoaderError) -> Self {
        Self::Loader(error)
    }
}

struct LoadedRecord {
    module: LoadedModule,
    registered: bool,
}

/// Persistent module state owned by the kernel.
pub struct ModuleRegistry {
    pool: Option<ModulePool>,
    modules: BTreeMap<u32, LoadedRecord>,
    next_id: u32,
    hooks: Option<ModuleHooks>,
}

impl ModuleRegistry {
    /// Create an empty registry.
    pub const fn new() -> Self {
        Self {
            pool: None,
            modules: BTreeMap::new(),
            next_id: 1,
            hooks: None,
        }
    }

    /// Install board/host runtime hooks.
    pub fn set_hooks(&mut self, hooks: ModuleHooks) {
        self.hooks = Some(hooks);
    }

    /// Remove runtime hooks (used by tests and secure teardown).
    pub fn clear_hooks(&mut self) {
        self.hooks = None;
    }

    /// Current runtime hooks, if installed.
    pub fn hooks(&self) -> Option<ModuleHooks> {
        self.hooks
    }

    /// Configure the persistent module pool. A second call is accepted only
    /// when it names the same range; otherwise the existing pool is kept.
    pub fn configure_pool(&mut self, base: u32, size: u32) -> Result<(), ModuleError> {
        if size == 0 || base == 0 {
            return Err(ModuleError::BadPool);
        }
        match self.pool {
            None => {
                self.pool = Some(ModulePool::new(base, size));
                Ok(())
            }
            Some(pool) if pool.base == base && pool.size == size => Ok(()),
            Some(_) => Err(ModuleError::PoolConflict),
        }
    }

    /// Whether a pool has been configured.
    pub fn has_pool(&self) -> bool {
        self.pool.is_some()
    }

    /// Bytes used in the module pool.
    pub fn pool_used(&self) -> u32 {
        self.pool.as_ref().map(|pool| pool.used()).unwrap_or(0)
    }

    /// Number of loaded modules (registered or not).
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Whether no modules are loaded.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Whether `id` exists and has been registered for execution.
    pub fn is_registered(&self, id: u32) -> bool {
        self.modules
            .get(&id)
            .map(|record| record.registered)
            .unwrap_or(false)
    }

    /// Load a module with the production (signature-enforcing) policy.
    ///
    /// `pool_base`/`pool_size` may be zero when the pool was configured by a
    /// previous call. On any failure the pool bump pointer is rolled back.
    pub fn load(
        &mut self,
        bytes: &[u8],
        loader_caps: &CSpace,
        pool_base: u32,
        pool_size: u32,
    ) -> Result<u32, ModuleError> {
        let hooks = self.hooks.ok_or(ModuleError::NoHooks)?;
        if pool_base != 0 || pool_size != 0 {
            self.configure_pool(pool_base, pool_size)?;
        }
        let pool = self.pool.as_mut().ok_or(ModuleError::BadPool)?;
        let checkpoint = pool.used();

        let module = match load_module(bytes, pool, loader_caps) {
            Ok(module) => module,
            Err(error) => {
                pool.rewind_to(checkpoint);
                return Err(ModuleError::Loader(error));
            }
        };

        if !(hooks.map)(module.base_paddr, module.image()) {
            pool.rewind_to(checkpoint);
            return Err(ModuleError::MapFailed);
        }

        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.modules.insert(
            id,
            LoadedRecord {
                module,
                registered: false,
            },
        );
        Ok(id)
    }

    /// Mark a loaded module as registered for execution.
    pub fn register(&mut self, id: u32) -> Result<(), ModuleError> {
        let record = self.modules.get_mut(&id).ok_or(ModuleError::NotFound)?;
        if record.registered {
            return Err(ModuleError::AlreadyRegistered);
        }
        record.registered = true;
        Ok(())
    }

    /// Invoke a registered module's entry point.
    pub fn start(&self, id: u32, arg: u32) -> Result<u32, ModuleError> {
        let record = self.modules.get(&id).ok_or(ModuleError::NotFound)?;
        if !record.registered {
            return Err(ModuleError::NotRegistered);
        }
        let hooks = self.hooks.ok_or(ModuleError::NoHooks)?;
        let entry = record.module.entry_vaddr;
        // SAFETY: the map hook wrote this image to `base_paddr` and the
        // board vouched for that range being executable. The module was
        // verified by `load` using the enforcing signature policy.
        Ok(unsafe { (hooks.execute)(entry, arg) })
    }

    /// Borrow a loaded module (used by board diagnostics).
    pub fn get(&self, id: u32) -> Option<&LoadedModule> {
        self.modules.get(&id).map(|record| &record.module)
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The syscall layer always enforces signatures.
pub const MODULE_SIGNATURE_POLICY: SignaturePolicy = SignaturePolicy::Enforce;
