//! Syscall dispatch
//!
//! Two entry points:
//!   - `syscall_dispatch` takes a 32-bit `SyscallArgs` (used by the
//!     real Cortex-M SVC exception).
//!   - `syscall_dispatch_wide` takes a `WideSyscallArgs` for tests
//!     on 64-bit hosts that need full pointer precision.

use crate::syscall::{Syscall, SyscallArgs, SyscallResult, WideSyscallArgs};
use crate::syscall_cap::{handle_cap_syscall, handle_cap_syscall_wide};
use crate::syscall_task::handle_task_syscall;
use crate::syscall_ipc::{handle_ipc_syscall, handle_ipc_syscall_wide};
use crate::syscall_m3::handle_m3_syscall;
use crate::syscall_crypto::handle_crypto_syscall;
use crate::syscall_flash::{handle_flash_read, handle_flash_write, handle_flash_erase};
use crate::syscall_timer::handle_timer_syscall;
use crate::syscall_memory::handle_memory_syscall;

/// Dispatch a syscall to the appropriate handler (32-bit args).
pub fn syscall_dispatch(args: SyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::TaskSpawn | Syscall::TaskExit | Syscall::TaskYield | Syscall::TaskGetId | Syscall::TaskSleep => {
            handle_task_syscall(args)
        }
        Syscall::CapDerive
        | Syscall::CapDuplicate
        | Syscall::CapRevoke
        | Syscall::CapFind
        | Syscall::CapGrant => handle_cap_syscall(args),
        Syscall::IpcSend
        | Syscall::IpcRecv
        | Syscall::IpcReply
        | Syscall::ChannelCreate
        | Syscall::ChannelSend
        | Syscall::ChannelRecv => handle_ipc_syscall(args),
        Syscall::MapMemory
        | Syscall::UnmapMemory
        | Syscall::AllocateMemory
        | Syscall::FreeMemory => {
            handle_memory_syscall(args)
        }
        Syscall::TimerGet | Syscall::TimerSet | Syscall::TimerCancel => {
            handle_timer_syscall(args)
        }
        Syscall::LogWrite => SyscallResult::ok(0),
        Syscall::NsCreate
        | Syscall::NsJoin
        | Syscall::NsPublish
        | Syscall::NsSubscribe
        | Syscall::NsEndpoint
        | Syscall::ModuleRegister
        | Syscall::ModuleStart
        | Syscall::ModuleLoad => handle_m3_syscall(args),
        Syscall::CryptoOp => handle_crypto_syscall(args),
        Syscall::FlashRead => handle_flash_read(args),
        Syscall::FlashWrite => handle_flash_write(args),
        Syscall::FlashErase => handle_flash_erase(args),
        Syscall::None => SyscallResult::err(1),
    }
}

/// Dispatch with wide (64-bit) arguments — used by host tests.
pub fn syscall_dispatch_wide(args: WideSyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::CapDerive
        | Syscall::CapDuplicate
        | Syscall::CapRevoke
        | Syscall::CapFind
        | Syscall::CapGrant => handle_cap_syscall_wide(args),
        Syscall::IpcSend
        | Syscall::IpcRecv
        | Syscall::IpcReply
        | Syscall::ChannelCreate
        | Syscall::ChannelSend
        | Syscall::ChannelRecv => handle_ipc_syscall_wide(args),
        _ => {
            // Fall back to 32-bit dispatch (truncate high bits).
            let narrow = SyscallArgs::new(args.number, args.arg0, args.arg1_lo, args.arg2_lo, args.arg3);
            syscall_dispatch(narrow)
        }
    }
}

/// Syscall dispatch function (called from assembly)
#[no_mangle]
pub extern "C" fn __caplet_syscall(args: *const SyscallArgs) -> SyscallResult {
    if args.is_null() {
        return SyscallResult::err(1);
    }
    let args = unsafe { *args };
    syscall_dispatch(args)
}