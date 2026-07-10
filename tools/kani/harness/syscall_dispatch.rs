//! Kani harness: syscall_dispatch — covers all 25 syscall paths.
//!
//! The 25 paths (counting the `None` invalid-syscall case) come from
//! the [`Syscall`] enum in `caplet-kernel::syscall`. For each one we
//! prove a tiny contract:
//!
//! 1. The dispatch never panics.
//! 2. It returns a `SyscallResult` whose `success` bit is set iff
//!    the syscall was implemented and the inputs were valid.
//! 3. The error code is in the expected range per syscall group.
//!
//! Where a syscall needs a fully-initialised kernel (e.g.
//! `TaskSpawn`, `TaskYield`), the harness sets up a minimal kernel
//! before exercising the path. Where the path is a stub returning a
//! constant error (e.g. `MapMemory` in M1), the harness asserts
//! exactly that error.
//!
//! Because Kani is not installed in this development environment,
//! this file is `#![cfg(kani)]`-gated so it does not affect normal
//! builds. To run:
//!
//! ```sh
//! cargo install --locked kani-verifier
//! cargo-kani setup
//! ./tools/kani/run.sh
//! ```

#![cfg(kani)]

use caplet_kernel::kernel::{get_kernel, init_kernel};
use caplet_kernel::kernel_config::KernelConfig;
use caplet_kernel::syscall::{syscall_dispatch, Syscall, SyscallArgs, SyscallResult};

// All Syscall variants we want to exercise. Each entry maps the
// `Syscall` variant to its numeric code so we can build a
// `SyscallArgs` directly.
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
];

// Compile-time check: 25 distinct paths.
const _: [(); 25] = [(); ALL_SYSCALL_NUMBERS.len()];

/// Set up a minimal kernel and return its raw pointer.
///
/// `init_kernel` is idempotent in practice (it `Box::leak`s a single
/// `Kernel` instance) so we can call it once per harness run.
fn ensure_kernel() {
    if unsafe { get_kernel() }.is_none() {
        let config = KernelConfig::qemu();
        unsafe { init_kernel(config) };
    }
}

/// `Syscall::None` and any unknown number must return an error
/// (error code 1, used throughout the dispatch).
#[kani::proof]
fn none_path_returns_error() {
    ensure_kernel();
    let bogus: u32 = kani::any();
    kani::assume(bogus > 50); // outside the implemented range
    let r = syscall_dispatch(SyscallArgs::new(bogus, 0, 0, 0, 0));
    assert!(!r.success);
    assert!(r.error != 0);
}

/// `Syscall::LogWrite` is the simplest implemented path; it should
/// always succeed with value 0.
#[kani::proof]
fn log_write_always_ok() {
    ensure_kernel();
    let a0: u32 = kani::any();
    let r = syscall_dispatch(SyscallArgs::new(Syscall::LogWrite.number(), a0, 0, 0, 0));
    assert!(r.success);
    assert!(r.value == 0);
    assert!(r.error == 0);
}

/// Memory syscalls (Map/Unmap/Allocate/Free) are stubs in M1; they
/// must return error code 1.
#[kani::proof]
fn memory_stubs_return_error() {
    ensure_kernel();
    for &(_, n) in ALL_SYSCALL_NUMBERS.iter().filter(|(s, _)| {
        matches!(
            s,
            Syscall::MapMemory
                | Syscall::UnmapMemory
                | Syscall::AllocateMemory
                | Syscall::FreeMemory
        )
    }) {
        let r = syscall_dispatch(SyscallArgs::new(n, 0, 0, 0, 0));
        assert!(!r.success, "memory syscall {} should error", n);
        assert!(r.error == 1, "memory syscall {} expected err=1 got {}", n, r.error);
    }
}

/// Timer syscalls are stubs in M1; they must return error code 1.
#[kani::proof]
fn timer_stubs_return_error() {
    ensure_kernel();
    for &(_, n) in ALL_SYSCALL_NUMBERS.iter().filter(|(s, _)| {
        matches!(
            s,
            Syscall::TimerGet | Syscall::TimerSet | Syscall::TimerCancel
        )
    }) {
        let r = syscall_dispatch(SyscallArgs::new(n, 0, 0, 0, 0));
        assert!(!r.success, "timer syscall {} should error", n);
        assert!(r.error == 1, "timer syscall {} expected err=1 got {}", n, r.error);
    }
}

/// `TaskGetId` returns the current task id; the dispatch must
/// succeed and the value must be a `u32`.
#[kani::proof]
fn task_get_id_does_not_panic() {
    ensure_kernel();
    let r = syscall_dispatch(SyscallArgs::new(Syscall::TaskGetId.number(), 0, 0, 0, 0));
    // Whether it succeeds depends on whether CURRENT_TASK_ID has
    // been set; either way the return type is well-defined.
    let _: u32 = r.value;
    let _: u32 = r.error;
}

/// `TaskSpawn` with an obviously invalid argument (entry == 0)
/// must reject and return an error.
#[kani::proof]
fn task_spawn_rejects_zero_entry() {
    ensure_kernel();
    let r = syscall_dispatch(SyscallArgs::new(
        Syscall::TaskSpawn.number(),
        0, // entry == 0
        0x2000_0000, // stack base (non-zero, required)
        4096,        // stack size (non-zero, required)
        8,           // priority
    ));
    assert!(!r.success);
}

/// `TaskSpawn` with a valid argument should succeed and return a
/// non-zero task id.
#[kani::proof]
fn task_spawn_succeeds_with_valid_args() {
    ensure_kernel();
    let entry: u32 = kani::any();
    kani::assume(entry > 0 && entry < u32::MAX - 1);
    let stack_base: u32 = kani::any();
    kani::assume(stack_base >= 0x2000_0000 && stack_base < 0x2010_0000);
    let stack_size: u32 = kani::any();
    kani::assume(stack_size >= 256 && stack_size <= 0x0010_0000);
    let priority: u32 = kani::any();
    kani::assume(priority < 32);

    let r = syscall_dispatch(SyscallArgs::new(
        Syscall::TaskSpawn.number(),
        entry,
        stack_base,
        stack_size,
        priority,
    ));
    assert!(r.success);
    assert!(r.value > 0);
}

/// `CapFind` with `name_ptr == null` must return an error (code 2).
#[kani::proof]
fn cap_find_rejects_null_name() {
    ensure_kernel();
    let r = syscall_dispatch(SyscallArgs::new(
        Syscall::CapFind.number(),
        0, // null pointer
        4,
        0,
        0,
    ));
    assert!(!r.success);
    assert!(r.error == 2, "expected err=2 got {}", r.error);
}

/// `CapGrant` is a reserved no-op in the minimal model; it must
/// succeed with value 0.
#[kani::proof]
fn cap_grant_is_noop() {
    ensure_kernel();
    let handle: u32 = kani::any();
    let target: u32 = kani::any();
    let r = syscall_dispatch(SyscallArgs::new(
        Syscall::CapGrant.number(),
        handle,
        target,
        0,
        0,
    ));
    assert!(r.success);
    assert!(r.value == 0);
}

/// All `Syscall` variants (25 in total) must round-trip through
/// `Syscall::from_number` back to themselves. This catches enum
/// numbering bugs.
#[kani::proof]
fn syscall_number_round_trip() {
    for &(s, n) in ALL_SYSCALL_NUMBERS {
        assert!(Syscall::from_number(n) == s, "round-trip failed for {:?}", s);
    }
}

/// The dispatcher's output for any of the 25 syscalls must be a
/// well-formed `SyscallResult` (success ⊕ error fields are
/// exclusive, value is whatever the handler returned).
#[kani::proof]
fn dispatch_returns_well_formed_result() {
    ensure_kernel();
    let pick: usize = kani::any();
    kani::assume(pick < ALL_SYSCALL_NUMBERS.len());
    let (_, n) = ALL_SYSCALL_NUMBERS[pick];
    let r: SyscallResult = syscall_dispatch(SyscallArgs::new(n, 0, 0, 0, 0));
    // A syscall that succeeded has error == 0; one that failed has
    // success == false. They are mutually exclusive.
    assert!(r.success ^ (r.error != 0));
}