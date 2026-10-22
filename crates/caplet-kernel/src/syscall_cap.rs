//! Capability syscall handlers
//!
//! These handlers take a `SyscallArgs` (number + 4 args) and operate on the
//! *current* task's CSpace. The current task is read from a global atomic
//! in the kernel (updated by the scheduler on every switch).
//! Errors are mapped from `CapError` into the syscall error space.

use crate::kernel::{with_current_cspace, with_kernel, CURRENT_TASK_ID};
use crate::quota::{self, CAPABILITY_SLOT_BYTES};
use crate::syscall::{Syscall, SyscallArgs, SyscallResult, WideSyscallArgs};
use ::caplet_cap::cspace::MAX_CSPACE_SLOTS;
use ::caplet_cap::error::CapError;
use ::caplet_cap::permissions::Permissions;
use ::caplet_cap::safety_bits::SafetyBits;

/// Map a `CapError` to a syscall error code.
fn cap_err_to_sys(e: CapError) -> SyscallResult {
    SyscallResult::err(e as u32 + 100)
}

fn budget_err_to_sys(error: caplet_budget::BudgetError) -> SyscallResult {
    use caplet_budget::BudgetError;
    let code = match error {
        BudgetError::MemoryExceeded { .. } => 60,
        BudgetError::CpuExceeded { .. } => 61,
        BudgetError::MessageRateExceeded { .. } => 62,
    };
    SyscallResult::err(code)
}

/// Handle capability-related syscalls
pub fn handle_cap_syscall(args: SyscallArgs) -> SyscallResult {
    let syscall = Syscall::from_number(args.number);
    // Handle indices are dense and bounded by the CSpace capacity.  Reject
    // impossible indices before touching kernel state: the lookup can never
    // succeed, and doing this up front avoids a CSpace lock (and, for bounded
    // verifiers, an unnecessary symbolic walk of the slot table) for every
    // out-of-range probe.
    if matches!(
        syscall,
        Syscall::CapDerive | Syscall::CapDuplicate | Syscall::CapRevoke | Syscall::CapGrant
    ) && args.arg0 as usize >= MAX_CSPACE_SLOTS
    {
        return match syscall {
            Syscall::CapDuplicate => SyscallResult::err(101),
            _ => cap_err_to_sys(CapError::NotFound),
        };
    }
    if matches!(syscall, Syscall::CapGrant) {
        // Grant is handled outside `with_current_cspace`: it needs to lock
        // the source process's CSpace and the destination process's CSpace,
        // and the kernel borrow must not overlap the source CSpace borrow.
        return cap_grant(args.arg0, args.arg1, args.arg2);
    }

    if matches!(syscall, Syscall::CapFind) {
        // Validate the user pointer before acquiring or cloning the CSpace.
        // Invalid lookups are common during probing and must not walk the
        // process table or touch capability state.
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
        let cspace = with_current_cspace(|cspace| cspace.clone());
        return match cspace.find_by_name(name) {
            Some(i) => SyscallResult::ok(i),
            None => SyscallResult::err(4),
        };
    }

    // Mapping does not touch capability storage.  Dispatch it before the
    // CSpace snapshot so rejected zero-sized mappings do not clone (or make
    // the verifier reason about) the whole capability table first.
    if matches!(syscall, Syscall::MapMemory) {
        return map_memory(args.arg0, args.arg1, args.arg2 as usize, args.arg3);
    }
    if matches!(syscall, Syscall::UnmapMemory) {
        return unmap_memory(args.arg0, args.arg1 as usize);
    }

    // A missing parent is a pure lookup failure.  Validate it before taking
    // the CSpace snapshot used by the allocating paths below; this keeps the
    // common invalid-handle path cheap and bounds verifier state.
    if matches!(syscall, Syscall::CapDerive) {
        let Some((parent_perms, parent_safety)) =
            with_current_cspace(|cspace| cspace.slot_perms_safety(args.arg0))
        else {
            return cap_err_to_sys(CapError::NotFound);
        };
        let parent = Permissions::from_bits(parent_perms.bits());
        if !parent.contains(::caplet_cap::permissions::perm::DERIVE) {
            return cap_err_to_sys(CapError::Permission);
        }
        let new_perms = Permissions::from_bits(args.arg1);
        if !new_perms.is_subset(parent) {
            return SyscallResult::err(50);
        }
        if !::caplet_cap::safety_policy::SafetyPolicy::validate_safety_bits(
            SafetyBits::from_bits(args.arg2 as u8),
            parent_safety,
        ) {
            return SyscallResult::err(51);
        }
    }
    if matches!(syscall, Syscall::CapDuplicate)
        && with_current_cspace(|cspace| cspace.slot_perms_safety(args.arg0)).is_none()
    {
        return SyscallResult::err(101);
    }

    // Snapshot the current CSpace handle without keeping a borrow of the
    // kernel alive.  Quota accounting calls back into `with_kernel`, so doing
    // it while holding `&mut Kernel` (as `with_current_cspace` would) creates
    // two overlapping `&mut Kernel` references.  A `CSpace` clone shares the
    // same locked table and is safe to use after the kernel borrow ends.
    let mut cspace = with_current_cspace(|cspace| cspace.clone());
    match syscall {
        Syscall::CapCreate => {
            // arg0 = permissions bits, arg1 = safety bits.  Root
            // capabilities have no parent and therefore do not require a
            // DERIVE right.  Only the kernel boot/loader path is expected
            // to expose this syscall to a process.
            let permissions = Permissions::from_bits(args.arg0);
            let safety = SafetyBits::from_bits(args.arg1 as u8);
            if let Err(error) = quota::charge_current_memory(CAPABILITY_SLOT_BYTES) {
                return budget_err_to_sys(error);
            }
            match cspace.allocate(permissions, safety, None) {
                Ok(h) => SyscallResult::ok(h.index()),
                Err(e) => {
                    quota::refund_current_memory(CAPABILITY_SLOT_BYTES);
                    cap_err_to_sys(e)
                }
            }
        }
        Syscall::CapDerive => {
            // arg0 = parent handle index
            // arg1 = permissions bits
            // arg2 = safety bits
            let parent_idx = args.arg0;
            let new_perms = Permissions::from_bits(args.arg1);
            let new_safety = SafetyBits::from_bits(args.arg2 as u8);

            // A derive must name a live parent.  In particular, slot 0 is
            // not a sentinel: callers that want a root use CapCreate.
            let Some((parent_perms, parent_safety)) = cspace.slot_perms_safety(parent_idx) else {
                return cap_err_to_sys(CapError::NotFound);
            };
            let parent = Permissions::from_bits(parent_perms.bits());
            if !parent.contains(::caplet_cap::permissions::perm::DERIVE) {
                return cap_err_to_sys(CapError::Permission);
            }
            if !new_perms.is_subset(parent) {
                return SyscallResult::err(50);
            }
            if !::caplet_cap::safety_policy::SafetyPolicy::validate_safety_bits(
                new_safety,
                parent_safety,
            ) {
                return SyscallResult::err(51);
            }
            if let Err(error) = quota::charge_current_memory(CAPABILITY_SLOT_BYTES) {
                return budget_err_to_sys(error);
            }
            match cspace.allocate(new_perms, new_safety, Some(parent_idx)) {
                Ok(h) => SyscallResult::ok(h.index()),
                Err(e) => {
                    quota::refund_current_memory(CAPABILITY_SLOT_BYTES);
                    cap_err_to_sys(e)
                }
            }
        }
        Syscall::CapDuplicate => {
            let parent_idx = args.arg0;
            let (perms, safety) = match cspace.slot_perms_safety(parent_idx) {
                Some(p) => p,
                None => return SyscallResult::err(101),
            };
            if let Err(error) = quota::charge_current_memory(CAPABILITY_SLOT_BYTES) {
                return budget_err_to_sys(error);
            }
            match cspace.allocate(perms, safety, Some(parent_idx)) {
                Ok(h) => SyscallResult::ok(h.index()),
                Err(e) => {
                    quota::refund_current_memory(CAPABILITY_SLOT_BYTES);
                    cap_err_to_sys(e)
                }
            }
        }
        Syscall::CapRevoke => {
            let idx = args.arg0;
            // Use slow-path revoke to invalidate descendants too.
            match ::caplet_cap::revocation::revoke_with_descendants(&mut cspace, idx) {
                Ok(r) => SyscallResult::ok(r.revoked_count as u32),
                Err(e) => cap_err_to_sys(e),
            }
        }
        Syscall::CapFind => unreachable!("CapFind is dispatched before the CSpace borrow"),
        Syscall::CapGrant => unreachable!("CapGrant is dispatched before the CSpace borrow"),
        Syscall::MapMemory | Syscall::UnmapMemory => {
            unreachable!("mapping syscalls are dispatched before the CSpace borrow")
        }
        _ => SyscallResult::err(1),
    }
}

/// Grant a capability to another task's process.
///
/// `source_index` names a live slot in the caller's CSpace.  The slot must
/// carry both the `TRANSFER` permission and the `TRANSFERABLE` safety bit;
/// this is deliberately checked in the kernel, not in user space.
/// `target_task` is a scheduler task id (`0` means the caller's task), and
/// `expected_generation` may be zero to accept the slot's current
/// generation or non-zero to pin the caller's view of the slot.
fn cap_grant(source_index: u32, target_task: u32, expected_generation: u32) -> SyscallResult {
    let source = with_current_cspace(|cspace| cspace.clone());
    let Some((permissions, safety)) = source.slot_perms_safety(source_index) else {
        return cap_err_to_sys(CapError::NotFound);
    };
    if !permissions.contains(::caplet_cap::permissions::perm::TRANSFER) {
        return cap_err_to_sys(CapError::Permission);
    }
    if !safety.transferable() {
        return cap_err_to_sys(CapError::Permission);
    }
    if expected_generation != 0 {
        let Some(generation) = source.slot_generation(source_index) else {
            return cap_err_to_sys(CapError::NotFound);
        };
        if generation.0 != expected_generation {
            return cap_err_to_sys(CapError::Revoked);
        }
    }

    let caller_task = CURRENT_TASK_ID.load(core::sync::atomic::Ordering::Acquire) as u32;
    let resolved_task = if target_task == 0 {
        caller_task
    } else {
        target_task
    };
    if resolved_task == 0 {
        return cap_err_to_sys(CapError::NotFound);
    }

    let Some(target_pid) = current_pid_for_task(resolved_task) else {
        return cap_err_to_sys(CapError::NotFound);
    };
    let result = with_kernel(|kernel| {
        let Some(process) = kernel.get_process_mut(target_pid) else {
            return Err(CapError::NotFound);
        };
        if process.state == crate::process::ProcessState::Terminated {
            return Err(CapError::Invalid);
        }
        if process.quota.charge_memory(CAPABILITY_SLOT_BYTES).is_err() {
            return Err(CapError::OutOfMemory);
        }
        match source.clone_slot_into(source_index, &mut process.cspace) {
            Ok(handle) => Ok(handle),
            Err(error) => {
                process.quota.refund_memory(CAPABILITY_SLOT_BYTES);
                Err(error)
            }
        }
    });
    match result {
        Some(Ok(handle)) => SyscallResult::ok(handle.index()),
        Some(Err(error)) => cap_err_to_sys(error),
        None => cap_err_to_sys(CapError::NotFound),
    }
}

fn current_pid_for_task(task_id: u32) -> Option<crate::process::ProcessId> {
    // `current_pid` only names the running task; grant must be able to
    // target any live process, so search the process table directly.
    with_kernel(|kernel| {
        kernel
            .processes
            .iter()
            .find_map(|(pid, process)| (process.task_id == Some(task_id)).then_some(*pid))
    })
    .flatten()
}

/// Wide-arg version for 64-bit host tests (pointer-safe).
pub fn handle_cap_syscall_wide(args: WideSyscallArgs) -> SyscallResult {
    let narrow = SyscallArgs::new(
        args.number,
        args.arg0,
        args.arg1_lo,
        args.arg2_lo,
        args.arg3,
    );
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
    use crate::mmu::{page_aligned_range, with_mmu, MmuOps, NoMmu, PagePerms};
    if page_aligned_range(vaddr, size).is_none() {
        return SyscallResult::err(32);
    }
    let perms = PagePerms {
        user_read: perms_bits & 1 != 0,
        user_write: perms_bits & 2 != 0,
        user_exec: perms_bits & 4 != 0,
        device: perms_bits & 8 != 0,
        shared: perms_bits & 0x10 != 0,
    };
    let result = with_mmu(|mmu| mmu.map(vaddr, paddr, size, perms));
    match result {
        Some(Ok(())) => SyscallResult::ok(0),
        Some(Err(_)) => SyscallResult::err(30),
        None => {
            // Cortex-M has no MMU. An identity mapping is the correct
            // semantics, but only after the range itself was checked.
            let mut mmu = NoMmu;
            match mmu.map(vaddr, paddr, size, perms) {
                Ok(()) => SyscallResult::ok(0),
                Err(_) => SyscallResult::err(30),
            }
        }
    }
}

fn unmap_memory(vaddr: u32, size: usize) -> SyscallResult {
    use crate::mmu::{page_aligned_range, with_mmu, MmuOps, NoMmu};
    if page_aligned_range(vaddr, size).is_none() {
        return SyscallResult::err(32);
    }
    match with_mmu(|mmu| mmu.unmap(vaddr, size)) {
        Some(Ok(())) => SyscallResult::ok(0),
        Some(Err(_)) => SyscallResult::err(31),
        None => {
            let mut mmu = NoMmu;
            match mmu.unmap(vaddr, size) {
                Ok(()) => SyscallResult::ok(0),
                Err(_) => SyscallResult::err(31),
            }
        }
    }
}
