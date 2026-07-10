//! Task syscall handlers
//!
//! These handlers drive the scheduler. They go through `Kernel::tick`
//! to make scheduling decisions, and through `Scheduler::block_current`
//! to suspend the calling task.

use crate::kernel::Kernel;
use crate::syscall::{Syscall, SyscallArgs, SyscallResult};
use core::sync::atomic::Ordering;

const ERR_NOT_INIT: u32 = 10;
const ERR_BAD_ARG: u32 = 11;

/// Get the current task ID (atomic load).
fn current_task_id() -> u32 {
    crate::kernel::CURRENT_TASK_ID.load(Ordering::Acquire) as u32
}

/// Handle task-related syscalls.
///
/// Syscall arguments are passed via the CPU's `SVC` exception frame;
/// the kernel reads them through `args`.
pub fn handle_task_syscall(args: SyscallArgs) -> SyscallResult {
    let kernel = unsafe { crate::kernel::get_kernel() };
    let kernel = match kernel {
        Some(k) => k,
        None => return SyscallResult::err(ERR_NOT_INIT),
    };

    match Syscall::from_number(args.number) {
        Syscall::TaskSpawn => {
            // arg0 = entry fn pointer (in app space)
            // arg1 = stack base address
            // arg2 = stack size
            // arg3 = priority
            let entry = args.arg0;
            let stack_base = args.arg1;
            let stack_size = args.arg2;
            let priority = args.arg3 as u8;
            if entry == 0 || stack_base == 0 || stack_size == 0 {
                return SyscallResult::err(ERR_BAD_ARG);
            }
            use crate::priority::Priority;
            use crate::scheduler::ScheduledTask;
            // Allocate a task ID. For now we use a simple counter.
            let id = next_task_id();
            let task = ScheduledTask::new(
                id,
                Priority::new(priority),
                entry,
                stack_base + stack_size, // top of stack
            )
            .with_stack_size(stack_size);
            kernel.add_task(task);
            SyscallResult::ok(id)
        }
        Syscall::TaskExit => {
            let id = current_task_id();
            if id == 0 {
                return SyscallResult::err(ERR_BAD_ARG);
            }
            // Mark the task as terminated and ask the scheduler to pick
            // another.
            let _ = kernel.scheduler().terminate(id);
            kernel.tick();
            // We never actually return to the caller in a real build —
            // the context switch takes us elsewhere. For host testing
            // we return ok.
            SyscallResult::ok(0)
        }
        Syscall::TaskYield => {
            // Mark current as yielding — the scheduler will pick a
            // different task (or the same one if it's the only one).
            let _ = kernel.scheduler().block_current();
            kernel.scheduler().make_ready(current_task_id());
            kernel.tick();
            SyscallResult::ok(0)
        }
        Syscall::TaskGetId => SyscallResult::ok(current_task_id()),
        Syscall::TaskSleep => {
            // For the minimal M1 implementation we treat sleep as
            // "yield" — there is no timer wheel yet (Phase 5).
            let _ = args.arg0;
            let _ = kernel.scheduler().block_current();
            kernel.tick();
            SyscallResult::ok(0)
        }
        _ => SyscallResult::err(1),
    }
}

fn next_task_id() -> u32 {
    use core::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(2); // 0 = idle, 1 = boot
    COUNTER.fetch_add(1, Ordering::Relaxed)
}
