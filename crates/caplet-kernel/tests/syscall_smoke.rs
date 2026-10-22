//! Integration tests for the real syscall handlers (Phase 0.3).
//!
//! These tests exercise the dispatch path end-to-end:
//!   - CapCreate / CapDerive / CapDuplicate / CapRevoke hit the real CSpace
//!   - TaskSpawn / TaskYield / TaskGetId hit the real scheduler
//!   - ChannelCreate / ChannelSend / ChannelRecv hit the real channel
//!     table
//!
//! On 64-bit hosts we use the `WideSyscallArgs` path so pointers are
//! not truncated.

extern crate alloc;

use ::caplet_kernel::syscall::{Syscall, SyscallResult, WideSyscallArgs};
use ::caplet_kernel::syscall_dispatch::syscall_dispatch_wide;

fn init_once() {
    // `Once` blocks every other test thread until the initialisation
    // closure has *returned*. An `AtomicBool::swap` guard is not
    // sufficient here: it publishes "done" before the work finishes, so
    // a thread that loses the race proceeds with an uninitialised (null)
    // kernel and sees spurious `ERR_NOT_INIT` failures.
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        // Touch the global CSpace once so it's leak-initialised. The
        // test binary's global allocator is the system one, so a
        // `Box::leak` works fine.
        ::caplet_kernel::kernel::with_current_cspace(|_| ());
        // Initialise the global kernel too so M3 syscalls that go
        // through `with_kernel` have something to talk to.
        unsafe {
            ::caplet_kernel::kernel::init_kernel(
                ::caplet_kernel::kernel_config::KernelConfig::default(),
            );
        }
    });
}

#[test]
fn cap_derive_and_revoke() {
    init_once();
    // Allocate a root cap through the explicit root-creation ABI.
    let parent = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::CapCreate.number(),
        0xFFFF_FFFF, // permissions
        0,           // safety
        0,
        0,
    ));
    assert!(parent.success, "first derive: {:?}", parent);
    let parent_idx = parent.value;

    // Derive a child with fewer perms.
    let child = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::CapDerive.number(),
        parent_idx,
        0x11, // READ | DERIVE
        0,
        0,
    ));
    assert!(child.success, "child derive: {:?}", child);
    let child_idx = child.value;
    assert_ne!(parent_idx, child_idx);

    // Revoke the parent.
    let rev = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::CapRevoke.number(),
        parent_idx,
        0,
        0,
        0,
    ));
    assert!(rev.success, "revoke: {:?}", rev);

    // Revocation is recursive, so duplicating the child must fail with
    // "revoked" rather than resurrecting a stale handle.
    let dup = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::CapDuplicate.number(),
        child_idx,
        0,
        0,
        0,
    ));
    assert!(
        !dup.success,
        "revoked child must not be duplicable: {:?}",
        dup
    );
}

#[test]
fn cap_derive_without_live_parent_is_rejected() {
    init_once();
    let r = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::CapDerive.number(),
        0xDEAD_BEEF,
        0x01,
        0,
        0,
    ));
    assert!(!r.success);
}

#[test]
fn task_get_id_returns_zero_or_init_error() {
    init_once();
    let r = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::TaskGetId.number(),
        0,
        0,
        0,
        0,
    ));
    // The kernel may or may not be initialized depending on whether
    // earlier tests ran the init path. If init'd, returns success.
    // Otherwise returns ERR_NOT_INIT (10). Either is acceptable for
    // this smoke test.
    if r.success {
        assert_eq!(r.value, 0);
    } else {
        assert_eq!(r.error, 10); // ERR_NOT_INIT
    }
}

#[test]
fn channel_roundtrip() {
    init_once();
    let create = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::ChannelCreate.number(),
        16,
        0,
        0,
        0,
    ));
    assert!(create.success, "channel create: {:?}", create);
    let cid = create.value;

    // Prepare a 5-byte payload in a static buffer (so the address is
    // stable across the call).
    static PAYLOAD: [u8; 5] = *b"hello";
    let payload_addr = PAYLOAD.as_ptr() as u64;
    let payload_len = PAYLOAD.len() as u32;

    let send = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::ChannelSend.number(),
        cid,
        payload_addr,
        payload_len as u64,
        0,
    ));
    assert!(send.success, "channel send: {:?}", send);

    // Receive into a 16-byte static buffer.
    static mut BUF: [u8; 16] = [0u8; 16];
    let buf_addr = core::ptr::addr_of_mut!(BUF) as u64;
    let recv = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::ChannelRecv.number(),
        cid,
        0,
        buf_addr,
        16,
    ));
    assert!(recv.success, "channel recv: {:?}", recv);
    assert_eq!(recv.value as usize, payload_len as usize);
    let received = unsafe {
        core::slice::from_raw_parts(core::ptr::addr_of!(BUF).cast::<u8>(), payload_len as usize)
    };
    assert_eq!(received, &PAYLOAD);
}

#[test]
fn channel_recv_empty_returns_would_block() {
    init_once();
    let create = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::ChannelCreate.number(),
        4,
        0,
        0,
        0,
    ));
    let cid = create.value;
    static mut BUF: [u8; 8] = [0u8; 8];
    let buf_addr = core::ptr::addr_of_mut!(BUF) as u64;
    let buf_len = 8u32;
    let r = syscall_dispatch_wide(WideSyscallArgs::new(
        Syscall::ChannelRecv.number(),
        cid,
        0,
        buf_addr,
        buf_len,
    ));
    assert!(!r.success);
    assert_eq!(r.error, 23); // ERR_WOULD_BLOCK
}

#[test]
fn invalid_syscall_returns_error() {
    init_once();
    let r: SyscallResult = syscall_dispatch_wide(WideSyscallArgs::new(
        9999, // not a real syscall
        0, 0, 0, 0,
    ));
    assert!(!r.success);
}

// M3 namespace and module coverage lives in dedicated serial integration
// tests (`ipc_namespace_security.rs` and `module_syscalls.rs`) because those
// operations mutate process-global kernel state.
