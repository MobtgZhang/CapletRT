//! Capability syscall handlers
//!
//! These handlers take a `SyscallArgs` (number + 4 args) and operate on the
//! *current* task's CSpace. The current task is read from a global atomic
//! in the kernel (updated by the scheduler on every switch).
//! Errors are mapped from `CapError` into the syscall error space.

use crate::kernel::with_current_cspace;
use crate::syscall::{Syscall, SyscallArgs, SyscallResult, WideSyscallArgs};
use ::caplet_cap::error::CapError;
use ::caplet_cap::permissions::Permissions;
use ::caplet_cap::safety_bits::SafetyBits;

/// Map a `CapError` to a syscall error code.
fn cap_err_to_sys(e: CapError) -> SyscallResult {
    SyscallResult::err(e as u32 + 100)
}

/// Handle capability-related syscalls
pub fn handle_cap_syscall(args: SyscallArgs) -> SyscallResult {
    with_current_cspace(|cspace| match Syscall::from_number(args.number) {
        Syscall::CapDerive => {
            // arg0 = parent handle index
            // arg1 = permissions bits
            // arg2 = safety bits
            let parent_idx = args.arg0;
            let new_perms = Permissions::from_bits(args.arg1);
            let new_safety = SafetyBits::from_bits(args.arg2 as u8);

            // Reject obviously-illegal derivations: deriving a child
            // whose permissions *exceed* the parent.
            if let Some((p_perm, _p_safety)) = cspace.slot_perms_safety(parent_idx) {
                // new_perms must be a subset of p_perm (cannot gain more permissions)
                // This checks: new_perms & ~p_perm == 0
                if !new_perms.is_subset(p_perm) {
                    return SyscallResult::err(50);
                }
            }
            match cspace.allocate(new_perms, new_safety, Some(parent_idx)) {
                Ok(h) => SyscallResult::ok(h.index()),
                Err(e) => cap_err_to_sys(e),
            }
        }
        Syscall::CapDuplicate => {
            let parent_idx = args.arg0;
            let (perms, safety) = match cspace.slot_perms_safety(parent_idx) {
                Some(p) => p,
                None => return SyscallResult::err(101),
            };
            match cspace.allocate(perms, safety, Some(parent_idx)) {
                Ok(h) => SyscallResult::ok(h.index()),
                Err(e) => cap_err_to_sys(e),
            }
        }
        Syscall::CapRevoke => {
            let idx = args.arg0;
            // Use slow-path revoke to invalidate descendants too.
            match ::caplet_cap::revocation::revoke_with_descendants(cspace, idx) {
                Ok(r) => SyscallResult::ok(r.revoked_count as u32),
                Err(e) => cap_err_to_sys(e),
            }
        }
        Syscall::CapFind => {
            let name_ptr = args.arg0 as *const u8;
            let name_len = args.arg1 as usize;
            if name_ptr.is_null() || name_len == 0 || name_len > 128 {
                return SyscallResult::err(2);
            }
            let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
            let name = match core::str::from_utf8(name_bytes) {
                Ok(s) => s,
                Err(_) => return SyscallResult::err(3),
            };
            match cspace.find_by_name(name) {
                Some(i) => SyscallResult::ok(i),
                None => SyscallResult::err(4),
            }
        }
        Syscall::CapGrant => {
            // arg0 = handle, arg1 = target task id.
            // In the minimal single-CSpace model this is a no-op
            // (we share the global CSpace). The interface is reserved
            // for Phase 4 multi-CSpace work.
            SyscallResult::ok(0)
        }
        Syscall::MapMemory => {
            // arg0 = vaddr, arg1 = paddr, arg2 = size, arg3 = perms bits.
            // Phase 3: this delegates to the active MmuOps
            // implementation (Cortex-A) or no-ops (Cortex-M).
            let vaddr = args.arg0;
            let paddr = args.arg1;
            let size = args.arg2 as usize;
            let perms_bits = args.arg3;
            map_memory(vaddr, paddr, size, perms_bits)
        }
        Syscall::UnmapMemory => {
            // arg0 = vaddr, arg1 = size.
            unmap_memory(args.arg0, args.arg1 as usize)
        }
        _ => SyscallResult::err(1),
    })
}

/// Wide-arg version for 64-bit host tests (pointer-safe).
pub fn handle_cap_syscall_wide(args: WideSyscallArgs) -> SyscallResult {
    let narrow = SyscallArgs::new(args.number, args.arg0, args.arg1_lo, args.arg2_lo, args.arg3);
    handle_cap_syscall(narrow)
}

/// Map `size` bytes at `vaddr` to physical `paddr` with the given
/// permission bit-pack. The caller (a process that holds a
/// `Cap<MemoryRegion>`) is expected to have passed through the
/// syscall cap-checker; this function does not re-check.
///
/// `perms_bits` matches [`PagePerms`] packed as a single u32:
///   bit 0: user_read
///   bit 1: user_write
///   bit 2: user_exec
///   bit 3: device (strong-ordering)
///   bit 4: shared
fn map_memory(vaddr: u32, paddr: u32, size: usize, perms_bits: u32) -> SyscallResult {
    use crate::mmu::{MmuOps, NoMmu, PagePerms};
    let perms = PagePerms {
        user_read: perms_bits & 1 != 0,
        user_write: perms_bits & 2 != 0,
        user_exec: perms_bits & 4 != 0,
        device: perms_bits & 8 != 0,
        shared: perms_bits & 0x10 != 0,
    };
    let mut mmu = NoMmu;
    match mmu.map(vaddr, paddr, size, perms) {
        Ok(()) => SyscallResult::ok(0),
        Err(_) => SyscallResult::err(30),
    }
}

fn unmap_memory(vaddr: u32, size: usize) -> SyscallResult {
    use crate::mmu::{MmuOps, NoMmu};
    let mut mmu = NoMmu;
    match mmu.unmap(vaddr, size) {
        Ok(()) => SyscallResult::ok(0),
        Err(_) => SyscallResult::err(31),
    }
}
