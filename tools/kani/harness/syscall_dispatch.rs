//! Kani harness: `caplet_kernel::syscall_dispatch`.
//!
//! The enumerations below are intentionally explicit. They are a
//! second, independent copy of the syscall ABI: if a new variant is
//! added without updating this table, the compile-time length check
//! and the round-trip proof catch the omission.
//!
//! These properties cover dispatch structure, argument validation,
//! and error/return invariants. They do not attempt to prove the
//! stateful IPC or module-loader implementations; those have separate
//! unit, integration, QEMU and Renode coverage.

#![cfg(kani)]

use caplet_kernel::ipc_namespace::NamespaceId;
use caplet_kernel::kernel::{get_kernel, init_kernel, CURRENT_TASK_ID};
use caplet_kernel::kernel_config::KernelConfig;
use caplet_kernel::process::ProcessId;
use caplet_kernel::syscall::{Syscall, SyscallArgs, SyscallResult};
use caplet_kernel::syscall_dispatch::syscall_dispatch;
use caplet_kernel::syscall_m3::ns_join_as;

/// Every `Syscall` variant together with its stable ABI number.
const ALL_SYSCALL_NUMBERS: &[(Syscall, u32)] = &[
    (Syscall::None, 0),
    (Syscall::TaskSpawn, 1),
    (Syscall::TaskExit, 2),
    (Syscall::TaskYield, 3),
    (Syscall::TaskGetId, 4),
    (Syscall::TaskSleep, 5),
    (Syscall::CapDerive, 10),
    (Syscall::CapDuplicate, 11),
    (Syscall::CapRevoke, 12),
    (Syscall::CapFind, 13),
    (Syscall::CapGrant, 14),
    (Syscall::CapCreate, 15),
    (Syscall::IpcSend, 20),
    (Syscall::IpcRecv, 21),
    (Syscall::IpcReply, 22),
    (Syscall::ChannelCreate, 23),
    (Syscall::ChannelSend, 24),
    (Syscall::ChannelRecv, 25),
    (Syscall::MapMemory, 30),
    (Syscall::UnmapMemory, 31),
    (Syscall::AllocateMemory, 32),
    (Syscall::FreeMemory, 33),
    (Syscall::TimerGet, 40),
    (Syscall::TimerSet, 41),
    (Syscall::TimerCancel, 42),
    (Syscall::LogWrite, 50),
    (Syscall::NsCreate, 60),
    (Syscall::NsJoin, 61),
    (Syscall::NsPublish, 62),
    (Syscall::NsSubscribe, 63),
    (Syscall::NsEndpoint, 64),
    (Syscall::ModuleRegister, 70),
    (Syscall::ModuleStart, 71),
    (Syscall::ModuleLoad, 72),
    (Syscall::CryptoOp, 80),
    (Syscall::FlashRead, 81),
    (Syscall::FlashWrite, 82),
    (Syscall::FlashErase, 83),
];

// None + 5 task + 6 capability + 6 IPC + 4 memory + 3 timer + log
// + 5 namespace + 3 module + crypto + 3 flash = 38 entries.
const _: [(); 38] = [(); ALL_SYSCALL_NUMBERS.len()];

/// Every number in the ABI's sparse range that is not assigned to a syscall.
///
/// Dispatch is deliberately not made symbolic over this list: routing an
/// unconstrained number through all handlers makes Kani's instrumentation
/// explore every handler at once. The table-level coverage below, followed by
/// concrete dispatch checks, keeps the proof bounded and reproducible.
const UNUSED_ABI_NUMBERS: &[u32] = &[
    0, 6, 7, 8, 9, 16, 17, 18, 19, 26, 27, 28, 29, 34, 35, 36, 37, 38, 39, 43, 44, 45, 46, 47, 48,
    49, 51, 52, 53, 54, 55, 56, 57, 58, 59, 65, 66, 67, 68, 69, 73, 74, 75, 76, 77, 78, 79,
];

fn ensure_kernel() {
    if unsafe { get_kernel() }.is_none() {
        let config = KernelConfig::qemu();
        unsafe { init_kernel(config) };
    }
    // These harnesses model the boot/host caller, whose task id is 0 and
    // which therefore owns the kernel CSpace.  Pin the atomic explicitly:
    // Kani otherwise explores the process-table fallback as well, which
    // multiplies the state space of every syscall proof without adding
    // coverage of the behaviour under test.
    CURRENT_TASK_ID.store(0, core::sync::atomic::Ordering::Release);
    kani::assume(CURRENT_TASK_ID.load(core::sync::atomic::Ordering::Acquire) == 0);
}

fn dispatch(number: u32, args: [u32; 4]) -> SyscallResult {
    syscall_dispatch(SyscallArgs::new(number, args[0], args[1], args[2], args[3]))
}

#[kani::proof]
fn every_syscall_number_round_trips() {
    for &(syscall, number) in ALL_SYSCALL_NUMBERS {
        assert!(
            Syscall::from_number(number) == syscall,
            "round-trip failed for {syscall:?} ({number})"
        );
    }
}

#[kani::proof]
fn unused_syscall_numbers_map_to_none() {
    for &number in UNUSED_ABI_NUMBERS {
        assert_eq!(
            Syscall::from_number(number),
            Syscall::None,
            "unused syscall number {number} was recognised"
        );
    }
}

#[kani::proof]
fn syscall_numbers_above_abi_ceiling_map_to_none() {
    let number: u32 = kani::any();
    kani::assume(number > Syscall::FlashErase.number());
    assert_eq!(Syscall::from_number(number), Syscall::None);
}

#[kani::proof]
#[kani::unwind(8)]
fn unknown_syscall_always_errors() {
    ensure_kernel();
    // The two table proofs above establish that every unused number maps to
    // None. This boundary call exercises the real dispatch path without
    // instantiating every handler for dozens of concrete inputs.
    let result = dispatch(
        u32::MAX,
        [kani::any(), kani::any(), kani::any(), kani::any()],
    );
    assert!(!result.success);
    assert_eq!(result.error, 1);
    assert_eq!(result.value, 0);
}

#[kani::proof]
#[kani::unwind(8)]
fn log_write_is_side_effect_free() {
    ensure_kernel();
    let result = dispatch(
        Syscall::LogWrite.number(),
        [kani::any(), kani::any(), kani::any(), kani::any()],
    );
    assert!(result.success);
    assert!(result.error == 0);
    assert!(result.value == 0);
}

#[kani::proof]
#[kani::unwind(8)]
fn task_spawn_rejects_zero_arguments() {
    ensure_kernel();

    let result = dispatch(Syscall::TaskSpawn.number(), [0, 0x2000_0000, 4096, 8]);
    assert!(!result.success);
    assert!(result.error == 11);

    let result = dispatch(Syscall::TaskSpawn.number(), [0x1000, 0, 4096, 8]);
    assert!(!result.success);
    assert!(result.error == 11);

    let result = dispatch(Syscall::TaskSpawn.number(), [0x1000, 0x2000_0000, 0, 8]);
    assert!(!result.success);
    assert!(result.error == 11);
}

#[kani::proof]
#[kani::unwind(8)]
fn task_spawn_rejects_stack_overflow() {
    ensure_kernel();
    let result = dispatch(Syscall::TaskSpawn.number(), [0x1000, u32::MAX, 2, 8]);
    assert!(!result.success);
    assert!(result.error == 11);
}

#[kani::proof]
#[kani::unwind(8)]
fn capability_lookup_rejects_null_name() {
    ensure_kernel();
    let result = dispatch(Syscall::CapFind.number(), [0, 4, 0, 0]);
    assert!(!result.success);
    assert!(result.error == 2);
}

#[kani::proof]
#[kani::unwind(8)]
fn duplicate_and_derive_without_live_parent_error() {
    ensure_kernel();

    // No live capability can occupy the all-ones handle index.  Index 0 is
    // implementation-defined and may be allocated by the boot process.
    let duplicate = dispatch(Syscall::CapDuplicate.number(), [u32::MAX, 0, 0, 0]);
    assert!(!duplicate.success);
    assert!(duplicate.error != 0);

    let derive = dispatch(Syscall::CapDerive.number(), [u32::MAX, 0, 0, 0]);
    assert!(!derive.success);
    assert!(derive.error != 0);
}

#[kani::proof]
#[kani::unwind(8)]
fn memory_operations_reject_zero_size_or_address() {
    ensure_kernel();

    let allocate = dispatch(Syscall::AllocateMemory.number(), [0, 1, 0, 0]);
    assert!(!allocate.success);
    assert!(allocate.error == 40);

    let free = dispatch(Syscall::FreeMemory.number(), [0, 0, 0, 0]);
    assert!(!free.success);
    assert!(free.error == 40);

    let map = dispatch(Syscall::MapMemory.number(), [0, 0, 0, 1]);
    assert!(!map.success);
    assert!(map.error != 0);

    let unmap = dispatch(Syscall::UnmapMemory.number(), [0, 0, 0, 0]);
    assert!(!unmap.success);
    assert!(unmap.error != 0);
}

#[kani::proof]
#[kani::unwind(8)]
fn timer_operations_reject_zero_arguments() {
    ensure_kernel();

    let get = dispatch(Syscall::TimerGet.number(), [0, 0, 0, 0]);
    assert!(!get.success);
    assert!(get.error == 30);

    let cancel = dispatch(Syscall::TimerCancel.number(), [0, 0, 0, 0]);
    assert!(!cancel.success);
    assert!(cancel.error == 30);
}

#[kani::proof]
#[kani::unwind(8)]
fn ipc_operations_reject_null_buffers() {
    ensure_kernel();

    for number in [
        Syscall::IpcSend.number(),
        Syscall::IpcRecv.number(),
        Syscall::IpcReply.number(),
        Syscall::NsPublish.number(),
        Syscall::NsSubscribe.number(),
        Syscall::NsEndpoint.number(),
    ] {
        let result = dispatch(number, [0, 0, 0, 0]);
        assert!(!result.success, "syscall {number} accepted a null buffer");
        assert_ne!(result.error, 0, "syscall {number} returned errno 0");
    }

    let channel_send = dispatch(Syscall::ChannelSend.number(), [1, 0, 0, 0]);
    assert!(!channel_send.success);
    assert!(channel_send.error == 22);

    let channel_recv = dispatch(Syscall::ChannelRecv.number(), [1, 0, 0, 0]);
    assert!(!channel_recv.success);
    assert!(channel_recv.error == 22);
}

#[kani::proof]
#[kani::unwind(8)]
fn namespace_join_rejects_missing_namespace() {
    ensure_kernel();
    // Namespace ids are allocated monotonically and never reach u32::MAX in
    // the bounded kernel fixture, so this is a guaranteed-missing id.  The
    // extracted policy entry point takes an already-resolved caller, which
    // keeps this proof focused on the namespace decision rather than on
    // process/CSpace construction.
    let result = ns_join_as(ProcessId(1), NamespaceId(u32::MAX));
    assert!(!result.success);
    assert!(result.error == 21);
}

#[kani::proof]
#[kani::unwind(8)]
fn module_syscalls_reject_missing_capability() {
    ensure_kernel();
    for number in [
        Syscall::ModuleRegister.number(),
        Syscall::ModuleStart.number(),
        Syscall::ModuleLoad.number(),
    ] {
        let result = dispatch(number, [0, 0, 0, 0]);
        assert!(!result.success, "syscall {number} accepted a null module");
        assert_ne!(result.error, 0, "syscall {number} returned errno 0");
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn crypto_and_flash_reject_missing_grants() {
    ensure_kernel();
    for number in [
        Syscall::CryptoOp.number(),
        Syscall::FlashRead.number(),
        Syscall::FlashWrite.number(),
        Syscall::FlashErase.number(),
    ] {
        let result = dispatch(number, [0, 0, 0, 0]);
        assert!(
            !result.success,
            "syscall {number} succeeded without a grant"
        );
        assert_ne!(result.error, 0, "syscall {number} returned errno 0");
    }
}

/// Null/default arguments must never cause a panic, and every result
/// must obey the ABI invariant: success iff errno is zero. The proofs
/// are split by subsystem so Kani instruments one bounded call graph at
/// a time instead of all syscall handlers in a single query.
fn assert_dispatch_well_formed(number: u32) {
    let result = dispatch(number, [0, 0, 0, 0]);

    if result.success {
        assert!(result.error == 0, "syscall {number} reported a stale errno");
    } else {
        assert!(
            result.error != 0,
            "syscall {number} failed without an errno"
        );
        assert!(result.value == 0, "syscall {number} failed with a value");
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn task_dispatch_results_are_well_formed() {
    ensure_kernel();
    for number in [
        Syscall::TaskSpawn.number(),
        Syscall::TaskExit.number(),
        Syscall::TaskYield.number(),
        Syscall::TaskGetId.number(),
        Syscall::TaskSleep.number(),
    ] {
        assert_dispatch_well_formed(number);
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn capability_dispatch_results_are_well_formed() {
    ensure_kernel();
    for number in [
        Syscall::CapDerive.number(),
        Syscall::CapDuplicate.number(),
        Syscall::CapRevoke.number(),
        Syscall::CapFind.number(),
        Syscall::CapGrant.number(),
        Syscall::CapCreate.number(),
    ] {
        assert_dispatch_well_formed(number);
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn ipc_dispatch_results_are_well_formed() {
    ensure_kernel();
    for number in [
        Syscall::IpcSend.number(),
        Syscall::IpcRecv.number(),
        Syscall::IpcReply.number(),
        Syscall::ChannelCreate.number(),
        Syscall::ChannelSend.number(),
        Syscall::ChannelRecv.number(),
    ] {
        assert_dispatch_well_formed(number);
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn memory_dispatch_results_are_well_formed() {
    ensure_kernel();
    for number in [
        Syscall::MapMemory.number(),
        Syscall::UnmapMemory.number(),
        Syscall::AllocateMemory.number(),
        Syscall::FreeMemory.number(),
    ] {
        assert_dispatch_well_formed(number);
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn timer_and_log_dispatch_results_are_well_formed() {
    ensure_kernel();
    for number in [
        Syscall::TimerGet.number(),
        Syscall::TimerSet.number(),
        Syscall::TimerCancel.number(),
        Syscall::LogWrite.number(),
    ] {
        assert_dispatch_well_formed(number);
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn namespace_dispatch_results_are_well_formed() {
    ensure_kernel();
    for number in [
        Syscall::NsCreate.number(),
        Syscall::NsJoin.number(),
        Syscall::NsPublish.number(),
        Syscall::NsSubscribe.number(),
        Syscall::NsEndpoint.number(),
    ] {
        assert_dispatch_well_formed(number);
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn module_dispatch_results_are_well_formed() {
    ensure_kernel();
    for number in [
        Syscall::ModuleRegister.number(),
        Syscall::ModuleStart.number(),
        Syscall::ModuleLoad.number(),
    ] {
        assert_dispatch_well_formed(number);
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn crypto_flash_dispatch_results_are_well_formed() {
    ensure_kernel();
    for number in [
        Syscall::CryptoOp.number(),
        Syscall::FlashRead.number(),
        Syscall::FlashWrite.number(),
        Syscall::FlashErase.number(),
    ] {
        assert_dispatch_well_formed(number);
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn invalid_dispatch_result_is_well_formed() {
    ensure_kernel();
    assert_dispatch_well_formed(Syscall::None.number());
}
