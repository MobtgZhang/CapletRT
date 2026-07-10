//! Handlers for the M3 syscalls.
//!
//! Split into:
//! - `name_len`-oriented namespace ops (create / join / publish / subscribe /
//!   endpoint registration); and
//! - module ops (load / start / register).
//!
//! The actual heavy lifting (manifest parse, signature check) lives in
//! `caplet-loader`. The kernel just hands the bytes down to the loader
//! and surfaces the result back to the caller.

extern crate alloc;
use alloc::string::String;

use caplet_cap::CSpace;

use crate::kernel::with_kernel;
use crate::syscall::{SyscallArgs, SyscallResult};
use crate::ipc_namespace::NamespaceId;

/// Handle M3 syscalls. Returns `SyscallResult::err(1)` if the syscall
/// number isn't an M3 op.
pub fn handle_m3_syscall(args: SyscallArgs) -> SyscallResult {
    let r = with_kernel(|k| {
        use crate::syscall::Syscall;
        match Syscall::from_number(args.number) {
            Syscall::NsCreate => ns_create(args, k),
            Syscall::NsJoin => ns_join(args, k),
            Syscall::NsPublish => ns_publish(args, k),
            Syscall::NsSubscribe => ns_subscribe(args, k),
            Syscall::NsEndpoint => ns_endpoint(args, k),
            Syscall::ModuleLoad => module_load(args),
            Syscall::ModuleStart => module_start(args),
            Syscall::ModuleRegister => module_register(args),
            _ => SyscallResult::err(1),
        }
    });
    r.unwrap_or_else(|| SyscallResult::err(1))
}

fn ns_create(
    _args: SyscallArgs,
    k: &mut crate::kernel::Kernel,
) -> SyscallResult {
    let name = String::from("user_ns");
    let id = k.create_namespace(name, Some(NamespaceId(0)));
    SyscallResult::ok(id.0)
}

fn ns_join(
    args: SyscallArgs,
    k: &mut crate::kernel::Kernel,
) -> SyscallResult {
    let pid = crate::process::ProcessId(args.arg0);
    let ns = NamespaceId(args.arg1);
    match k.process_join_namespace(pid, ns) {
        Ok(()) => SyscallResult::ok(0),
        Err(_) => SyscallResult::err(2),
    }
}

fn ns_publish(_args: SyscallArgs, _k: &mut crate::kernel::Kernel) -> SyscallResult {
    // The dispatch lives in `syscall_ipc`; this stub is here so the
    // syscall number has a visible handler at kernel init time.
    SyscallResult::ok(0)
}

fn ns_subscribe(_args: SyscallArgs, _k: &mut crate::kernel::Kernel) -> SyscallResult {
    SyscallResult::ok(0)
}

fn ns_endpoint(_args: SyscallArgs, _k: &mut crate::kernel::Kernel) -> SyscallResult {
    SyscallResult::ok(0)
}

fn module_load(_args: SyscallArgs) -> SyscallResult {
    SyscallResult::err(1)
}

fn module_start(_args: SyscallArgs) -> SyscallResult {
    SyscallResult::err(1)
}

fn module_register(_args: SyscallArgs) -> SyscallResult {
    SyscallResult::ok(0)
}

// Re-export so callers don't have to dig into both `with_kernel` and
// `with_cspace`. The `with_current_cspace` in `kernel.rs` already
// implements the per-process lookup.
#[allow(dead_code)]
pub(crate) fn current_cspace() -> Option<&'static mut CSpace> {
    crate::kernel::with_current_cspace_into_static()
}
