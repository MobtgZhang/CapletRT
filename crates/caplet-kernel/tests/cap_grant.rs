//! End-to-end tests for `CapGrant`.
//!
//! This binary intentionally contains one sequential test: the kernel is a
//! leaked global whose current-task register and process table are shared by
//! every test in the process.

extern crate alloc;

use alloc::string::String;
use caplet_budget::Budget;
use caplet_cap::handle::Handle;
use caplet_kernel::kernel::{init_kernel, with_current_cspace, with_kernel, CURRENT_TASK_ID};
use caplet_kernel::kernel_config::KernelConfig;
use caplet_kernel::priority::Priority;
use caplet_kernel::process::{Process, ProcessId, ProcessState};
use caplet_kernel::scheduler::ScheduledTask;
use caplet_kernel::syscall::{Syscall, SyscallResult, WideSyscallArgs};
use caplet_kernel::syscall_dispatch::syscall_dispatch_wide;
use core::sync::atomic::Ordering;

const READ: u32 = 0x1;
const TRANSFER: u32 = 1 << 5;
const TRANSFERABLE: u32 = 1 << 3;

fn init() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| unsafe {
        init_kernel(KernelConfig::default());
    });
}

fn create_root(permissions: u32, safety: u32) -> SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::CapCreate.number(),
        permissions,
        u64::from(safety),
        0,
        0,
    ))
}

fn grant(source: u32, task: u32, generation: u32) -> SyscallResult {
    syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::CapGrant.number(),
        source,
        u64::from(task),
        u64::from(generation),
        0,
    ))
}

fn add_process(task_id: u32, budget: Budget) -> ProcessId {
    with_kernel(|kernel| {
        let pid = kernel.create_process(
            Process::new(ProcessId(0), String::from("grant-target"), 0, 0, 4096)
                .with_budget(budget),
        );
        kernel
            .add_task_for_process(pid, ScheduledTask::new(task_id, Priority::new(1), 0, 0))
            .expect("bind grant target task");
        pid
    })
    .expect("kernel initialised")
}

fn target_capacity(pid: ProcessId) -> usize {
    with_kernel(|kernel| kernel.get_process(pid).unwrap().cspace.active_count())
        .expect("kernel initialised")
}

fn target_quota_used(pid: ProcessId) -> u32 {
    with_kernel(|kernel| kernel.get_process(pid).unwrap().quota.memory_used())
        .expect("kernel initialised")
}

#[test]
fn cap_grant_kernel_semantics() {
    init();
    CURRENT_TASK_ID.store(0, Ordering::Release);

    let source = create_root(READ | TRANSFER, TRANSFERABLE);
    assert!(source.success, "create grantable root: {source:?}");

    // Success path: the destination process receives a live slot, and the
    // pinned generation is accepted.
    let target_pid = add_process(101, Budget::default());
    let generation = with_current_cspace(|cspace| cspace.slot_generation(source.value))
        .expect("source generation");
    let granted = grant(source.value, 101, generation.0);
    assert!(granted.success, "grant should succeed: {granted:?}");
    assert_eq!(target_capacity(target_pid), 1);
    assert_eq!(target_quota_used(target_pid), 64);
    let target_generation = with_kernel(|kernel| {
        kernel
            .get_process(target_pid)
            .unwrap()
            .cspace
            .slot_generation(granted.value)
            .expect("granted slot generation")
    })
    .expect("kernel initialised");
    let target_slot = with_kernel(|kernel| {
        kernel
            .get_process(target_pid)
            .unwrap()
            .cspace
            .lookup(Handle::<u32>::new(granted.value, target_generation))
            .is_ok()
    })
    .expect("kernel initialised");
    assert!(target_slot, "granted handle must resolve in target CSpace");

    // A live but non-transferable capability is rejected by the kernel
    // safety check even when it carries TRANSFER.
    let no_transfer_bit = create_root(READ | TRANSFER, 0);
    assert!(no_transfer_bit.success);
    assert_eq!(
        grant(no_transfer_bit.value, 101, 0).error,
        101,
        "non-transferable capability must be refused"
    );

    // A capability without the TRANSFER permission is also refused.
    let no_transfer_perm = create_root(READ, TRANSFERABLE);
    assert!(no_transfer_perm.success);
    assert_eq!(grant(no_transfer_perm.value, 101, 0).error, 101);

    assert_eq!(grant(0xDEAD_BEEF, 101, 0).error, 100);
    assert_eq!(grant(source.value, 0xDEAD_BEEF, 0).error, 100);
    assert_eq!(
        grant(source.value, 101, generation.0.wrapping_add(1)).error,
        105
    );

    // Terminated targets cannot receive new authority.
    let terminated_pid = add_process(102, Budget::default());
    with_kernel(|kernel| kernel.get_process_mut(terminated_pid).unwrap().terminate())
        .expect("kernel initialised");
    assert_eq!(grant(source.value, 102, 0).error, 102);

    // A zero-memory budget rejects before the slot is allocated.
    let no_memory_budget = Budget {
        memory_bytes: 0,
        ..Budget::default()
    };
    let exhausted_pid = add_process(103, no_memory_budget);
    assert_eq!(grant(source.value, 103, 0).error, 103);
    assert_eq!(target_capacity(exhausted_pid), 0);
    assert_eq!(target_quota_used(exhausted_pid), 0);

    // Fail after charging quota (destination table full) must refund the
    // charge.  Fill all 256 slots with metadata-only roots.
    let full_pid = add_process(104, Budget::default());
    with_kernel(|kernel| {
        let process = kernel.get_process_mut(full_pid).unwrap();
        for _ in 0..256 {
            process
                .cspace
                .allocate(
                    caplet_cap::permissions::Permissions::all(),
                    caplet_cap::safety_bits::SafetyBits::default(),
                    None,
                )
                .unwrap();
        }
    })
    .expect("kernel initialised");
    assert_eq!(target_capacity(full_pid), 256);
    assert_eq!(grant(source.value, 104, 0).error, 109);
    assert_eq!(target_quota_used(full_pid), 0, "failed grant must refund");

    // Same-CSpace grants are rejected instead of deadlocking on the table
    // lock.  Make the caller's current task belong to `same_pid`.
    let _same_pid = add_process(105, Budget::default());
    CURRENT_TASK_ID.store(105, Ordering::Release);
    let same_source = create_root(READ | TRANSFER, TRANSFERABLE);
    assert!(same_source.success);
    assert_eq!(grant(same_source.value, 105, 0).error, 102);
    CURRENT_TASK_ID.store(0, Ordering::Release);

    // The original target remains usable while the source is live.
    assert_eq!(target_capacity(target_pid), 1);
    assert!(with_kernel(
        |kernel| kernel.get_process(target_pid).unwrap().state != ProcessState::Terminated
    )
    .expect("kernel initialised"));
}
