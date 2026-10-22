//! Handlers for namespace lifecycle and module-management syscalls.
//!
//! Endpoint registration, publish, and subscribe live in
//! [`crate::syscall_ipc`]; this module owns only operations that create or
//! mutate kernel-owned objects.

extern crate alloc;
use alloc::string::String;

use crate::ipc_namespace::NamespaceId;
use crate::kernel::{current_pid, with_current_cspace, with_kernel};
use crate::process::ProcessId;
use crate::syscall::{Syscall, SyscallArgs, SyscallResult, WideSyscallArgs};
use caplet_cap::permissions::perm;
use caplet_cap::SafetyBits;

use crate::syscall_ipc::{
    ERR_BAD_ARG, ERR_DENIED, ERR_EXISTS, ERR_NOT_FOUND, ERR_NOT_INIT, ERR_NO_SPACE,
};

const MAX_NAMESPACE_NAME: usize = 256;
const MAX_MODULE_IMAGE: usize = 16 * 1024 * 1024;
const NS_CAP_NAME_BUF: usize = 16;

/// Render the kernel-owned capability name for a namespace without touching
/// the heap.  Namespace joins run on the syscall hot path, and capacity
/// strings are limited to `"ns:"` plus a ten-digit `u32`.
fn namespace_cap_name(namespace: NamespaceId, buf: &mut [u8; NS_CAP_NAME_BUF]) -> &str {
    let mut digits = [0u8; 10];
    let mut value = namespace.0;
    let mut len = 0;
    loop {
        digits[len] = b'0' + (value % 10) as u8;
        len += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    buf[0] = b'n';
    buf[1] = b's';
    buf[2] = b':';
    let mut index = 0;
    while index < len {
        buf[3 + index] = digits[len - 1 - index];
        index += 1;
    }
    core::str::from_utf8(&buf[..3 + len]).unwrap_or("")
}

/// No board/host runtime hooks are installed for module loading.
pub const ERR_NOT_READY: u32 = 31;
/// The module failed signature verification or the enforcing policy.
pub const ERR_MODULE_SIGNATURE: u32 = 32;
/// The module container could not be parsed.
pub const ERR_MODULE_PARSE: u32 = 33;
/// The module pool is exhausted or the image size is invalid.
pub const ERR_MODULE_MEMORY: u32 = 34;
/// The module entry point is outside the loaded image.
pub const ERR_MODULE_ENTRY: u32 = 35;
/// The module exists but has not been registered for execution.
pub const ERR_NOT_REGISTERED: u32 = 36;
/// The board could not map the module image.
pub const ERR_MODULE_MAP: u32 = 37;
/// The module pool range is invalid or conflicts with the existing pool.
pub const ERR_MODULE_POOL: u32 = 38;
/// A relocation inside the module was invalid.
pub const ERR_MODULE_RELOC: u32 = 39;

/// Handle M3 syscalls. Returns `SyscallResult::err(1)` if the syscall
/// number isn't an M3 op.
pub fn handle_m3_syscall(args: SyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::NsCreate => {
            let name = match read_name(args.arg0 as usize, args.arg1 as usize) {
                Ok(name) => name,
                Err(error) => return SyscallResult::err(error),
            };
            ns_create(name, args.arg2)
        }
        Syscall::NsJoin => ns_join(NamespaceId(args.arg0)),
        Syscall::ModuleLoad | Syscall::ModuleStart | Syscall::ModuleRegister => {
            handle_module_syscall(args)
        }
        _ => SyscallResult::err(1),
    }
}

/// Wide-argument entry point for namespace lifecycle and module syscalls.
pub fn handle_m3_syscall_wide(args: WideSyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::NsCreate => {
            let name = match read_name(
                read_wide_ptr(args.arg1_lo, args.arg1_hi),
                args.arg0 as usize,
            ) {
                Ok(name) => name,
                Err(error) => return SyscallResult::err(error),
            };
            ns_create(name, args.arg2_lo)
        }
        Syscall::NsJoin => ns_join(NamespaceId(args.arg0)),
        Syscall::ModuleLoad | Syscall::ModuleStart | Syscall::ModuleRegister => {
            handle_module_syscall_wide(args)
        }
        _ => SyscallResult::err(1),
    }
}

fn read_wide_ptr(lo: u32, hi: u32) -> usize {
    (((hi as u64) << 32) | lo as u64) as usize
}

fn read_name(ptr: usize, len: usize) -> Result<String, u32> {
    if ptr == 0 || len == 0 || len > MAX_NAMESPACE_NAME {
        return Err(ERR_BAD_ARG);
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    let text = core::str::from_utf8(bytes).map_err(|_| ERR_BAD_ARG)?;
    if text.chars().any(char::is_control) {
        return Err(ERR_BAD_ARG);
    }
    Ok(String::from(text))
}

fn has_named_cap(name: &str) -> bool {
    with_current_cspace(|cspace| {
        let Some(index) = cspace.find_by_name(name) else {
            return false;
        };
        cspace
            .slot_perms_safety(index)
            .map(|(permissions, _)| permissions.contains(perm::READ))
            .unwrap_or(false)
    })
}

fn current_namespace() -> NamespaceId {
    current_pid()
        .and_then(|pid| {
            with_kernel(|kernel| kernel.get_process(pid).map(|process| process.ipc_namespace))
                .flatten()
        })
        .unwrap_or(NamespaceId(0))
}

fn namespace_exists(namespace: NamespaceId) -> bool {
    with_kernel(|kernel| kernel.namespaces.get(namespace).is_some()).unwrap_or(false)
}

fn install_namespace_capability(namespace: NamespaceId) -> Result<u32, u32> {
    let mut name_buf = [0u8; NS_CAP_NAME_BUF];
    with_current_cspace(|cspace| {
        let handle = cspace
            .allocate(perm::READ | perm::DERIVE, SafetyBits::default(), None)
            .map_err(|_| ERR_NO_SPACE)?;
        cspace
            .bind_name(handle.index(), namespace_cap_name(namespace, &mut name_buf))
            .map_err(|_| ERR_NO_SPACE)?;
        Ok(handle.index())
    })
}

fn ns_create(name: String, parent_arg: u32) -> SyscallResult {
    if !has_named_cap("ns:create") {
        return SyscallResult::err(ERR_DENIED);
    }
    let parent = if parent_arg == u32::MAX {
        current_namespace()
    } else {
        NamespaceId(parent_arg)
    };
    if !namespace_exists(parent) {
        return SyscallResult::err(ERR_NOT_FOUND);
    }
    let id =
        with_kernel(|kernel| kernel.create_namespace(name, Some(parent))).unwrap_or(NamespaceId(0));
    if id == NamespaceId(0) {
        return SyscallResult::err(1);
    }
    if let Err(error) = install_namespace_capability(id) {
        with_kernel(|kernel| {
            kernel.namespaces.remove(id);
        });
        return SyscallResult::err(error);
    }
    SyscallResult::ok(id.0)
}

fn ns_join(namespace: NamespaceId) -> SyscallResult {
    let Some(pid) = current_pid() else {
        return SyscallResult::err(ERR_DENIED);
    };
    ns_join_as(pid, namespace)
}

/// Join `namespace` on behalf of a caller whose process id has already been
/// resolved.
///
/// Keeping the policy check separate from `current_pid()` lets the bounded
/// verification harness exercise the missing-namespace decision without
/// constructing a full process/CSpace graph, which is unrelated to this
/// predicate.  `pid` must come from a validated syscall caller.
pub fn ns_join_as(pid: ProcessId, namespace: NamespaceId) -> SyscallResult {
    if !namespace_exists(namespace) {
        return SyscallResult::err(ERR_NOT_FOUND);
    }
    let mut name_buf = [0u8; NS_CAP_NAME_BUF];
    if !has_named_cap(namespace_cap_name(namespace, &mut name_buf)) {
        return SyscallResult::err(ERR_DENIED);
    }
    match with_kernel(|kernel| kernel.process_join_namespace(pid, namespace)) {
        Some(Ok(())) => SyscallResult::ok(0),
        Some(Err(_)) => SyscallResult::err(ERR_NOT_FOUND),
        None => SyscallResult::err(1),
    }
}

fn handle_module_syscall(args: SyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::ModuleLoad => {
            module_load(args.arg0 as usize, args.arg1 as usize, args.arg2, args.arg3)
        }
        Syscall::ModuleRegister => module_register(args.arg0),
        Syscall::ModuleStart => module_start(args.arg0, args.arg1),
        _ => SyscallResult::err(1),
    }
}

fn handle_module_syscall_wide(args: WideSyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::ModuleLoad => module_load(
            read_wide_ptr(args.arg1_lo, args.arg1_hi),
            args.arg0 as usize,
            args.arg2_lo,
            args.arg3,
        ),
        Syscall::ModuleRegister => module_register(args.arg0),
        Syscall::ModuleStart => module_start(args.arg0, args.arg1_lo),
        _ => SyscallResult::err(1),
    }
}

fn module_load(ptr: usize, len: usize, pool_base: u32, pool_size: u32) -> SyscallResult {
    if !has_named_cap("module:load") {
        return SyscallResult::err(ERR_DENIED);
    }
    if ptr == 0 || len == 0 || len > MAX_MODULE_IMAGE {
        return SyscallResult::err(ERR_BAD_ARG);
    }
    // SAFETY: the syscall ABI requires the caller to provide a readable
    // image buffer for the duration of this call.
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };

    #[cfg(feature = "module-loader")]
    {
        let pid = current_pid();
        let result = with_kernel(|kernel| {
            let loader_caps = match pid {
                Some(pid) => match kernel.processes.get(pid) {
                    Some(process) => &process.cspace,
                    None => return Err(ERR_NOT_FOUND),
                },
                None => &kernel.cspace,
            };
            kernel
                .modules
                .load(bytes, loader_caps, pool_base, pool_size)
                .map_err(module_error)
        });
        match result {
            Some(Ok(id)) => SyscallResult::ok(id),
            Some(Err(error)) => SyscallResult::err(error),
            None => SyscallResult::err(ERR_NOT_INIT),
        }
    }
    #[cfg(not(feature = "module-loader"))]
    {
        let _ = (bytes, pool_base, pool_size);
        SyscallResult::err(ERR_NOT_READY)
    }
}

fn module_register(id: u32) -> SyscallResult {
    if !has_named_cap("module:register") {
        return SyscallResult::err(ERR_DENIED);
    }
    #[cfg(feature = "module-loader")]
    {
        match with_kernel(|kernel| kernel.modules.register(id).map_err(module_error)) {
            Some(Ok(())) => SyscallResult::ok(0),
            Some(Err(error)) => SyscallResult::err(error),
            None => SyscallResult::err(ERR_NOT_INIT),
        }
    }
    #[cfg(not(feature = "module-loader"))]
    {
        let _ = id;
        SyscallResult::err(ERR_NOT_READY)
    }
}

fn module_start(id: u32, arg: u32) -> SyscallResult {
    if !has_named_cap("module:start") {
        return SyscallResult::err(ERR_DENIED);
    }
    #[cfg(feature = "module-loader")]
    {
        match with_kernel(|kernel| kernel.modules.start(id, arg).map_err(module_error)) {
            Some(Ok(value)) => SyscallResult::ok(value),
            Some(Err(error)) => SyscallResult::err(error),
            None => SyscallResult::err(ERR_NOT_INIT),
        }
    }
    #[cfg(not(feature = "module-loader"))]
    {
        let _ = (id, arg);
        SyscallResult::err(ERR_NOT_READY)
    }
}

#[cfg(feature = "module-loader")]
fn module_error(error: crate::module::ModuleError) -> u32 {
    use crate::module::ModuleError;
    use caplet_loader::sandbox::LoaderError;

    match error {
        ModuleError::NoHooks => ERR_NOT_READY,
        ModuleError::NotFound => ERR_NOT_FOUND,
        ModuleError::AlreadyRegistered => ERR_EXISTS,
        ModuleError::NotRegistered => ERR_NOT_REGISTERED,
        ModuleError::PoolConflict | ModuleError::BadPool => ERR_MODULE_POOL,
        ModuleError::MapFailed => ERR_MODULE_MAP,
        ModuleError::Loader(loader) => match loader {
            LoaderError::BadMagic | LoaderError::ParseFailed => ERR_MODULE_PARSE,
            LoaderError::BadSignature
            | LoaderError::UnsignedModule
            | LoaderError::CryptoUnavailable => ERR_MODULE_SIGNATURE,
            LoaderError::OutOfMemory | LoaderError::BadSize => ERR_MODULE_MEMORY,
            LoaderError::BadReloc => ERR_MODULE_RELOC,
            LoaderError::BadEntry => ERR_MODULE_ENTRY,
            LoaderError::MissingCapability => ERR_DENIED,
        },
    }
}
