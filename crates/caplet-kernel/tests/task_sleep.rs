//! Task sleep must put the caller to sleep until its timer expires,
//! rather than behaving like an immediate yield.

use caplet_kernel::kernel::{get_kernel, init_kernel, CURRENT_TASK_ID};
use caplet_kernel::kernel_config::KernelConfig;
use caplet_kernel::priority::Priority;
use caplet_kernel::scheduler::ScheduledTask;
use caplet_kernel::syscall::{Syscall, SyscallArgs};
use caplet_kernel::syscall_task::handle_task_syscall;
use caplet_kernel::syscall_timer::{active_timer_count, get_ticks, increment_ticks};

#[test]
fn task_sleep_wakes_at_deadline() {
    unsafe { init_kernel(KernelConfig::default()) };
    let kernel = unsafe { get_kernel() }.expect("kernel initialized");

    kernel.add_task(ScheduledTask::new(101, Priority::new(1), 0, 0));
    kernel.add_task(ScheduledTask::new(102, Priority::new(5), 0, 0));
    assert!(matches!(
        kernel.tick(),
        caplet_kernel::scheduler::ScheduleOutcome::Switch { next: 101, .. }
    ));
    CURRENT_TASK_ID.store(101, core::sync::atomic::Ordering::Release);

    let before = active_timer_count();
    let start = get_ticks();
    let result = handle_task_syscall(SyscallArgs::new(Syscall::TaskSleep.number(), 5, 0, 0, 0));
    assert!(result.success, "sleep syscall failed: {result:?}");
    assert_eq!(active_timer_count(), before + 1);
    assert_eq!(kernel.scheduler().ready_len(), 0);
    assert_eq!(kernel.scheduler().current_task().map(|t| t.id), Some(102));

    // Four of five milliseconds have elapsed: the sleeper is still
    // blocked and only the other task is running.
    increment_ticks(4);
    assert_eq!(get_ticks(), start + 4);
    assert_eq!(active_timer_count(), before + 1);
    assert_eq!(kernel.scheduler().ready_len(), 0);

    // The fifth tick expires the timer and makes task 101 runnable.
    increment_ticks(1);
    assert_eq!(get_ticks(), start + 5);
    assert_eq!(active_timer_count(), before);
    assert_eq!(kernel.scheduler().ready_len(), 1);
    assert!(kernel.scheduler().task(101).is_some());
}
