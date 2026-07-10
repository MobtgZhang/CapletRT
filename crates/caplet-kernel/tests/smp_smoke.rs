//! SMP scheduler unit tests.
//!
//! These tests simulate multiple logical CPUs on the host and
//! verify that the `SmpScheduler` behaves correctly under the
//! constraints docv1.md §13.3 promises:
//!
//! - a task is never given to two CPUs simultaneously
//! - `migrate(task, dst)` updates `cpu_of(task)` correctly
//! - the load balancer migrates a task when imbalance > threshold

use caplet_kernel::cpu::{set_cpu_count, CpuId};
use caplet_kernel::priority::Priority;
use caplet_kernel::scheduler::ScheduledTask;
use caplet_kernel::smp_scheduler::{AffinityMask, SmpScheduler};

fn task(id: u32, prio: u8) -> ScheduledTask {
    ScheduledTask::new(id, Priority::new(prio), 0, 0)
}

#[test]
fn scheduler_balances_across_cores() {
    set_cpu_count(4);
    let mut s = SmpScheduler::new(4, &[100, 101, 102, 103]);
    for i in 0..10 {
        s.add_task(task(i + 1, 8));
    }
    let snap = s.snapshot();
    let total = snap.iter().map(|c| c.run_queue_len).sum::<usize>();
    assert_eq!(total, 10);
    // The maximum single-CPU load should be much less than 10 after
    // a balance pass.
    assert!(snap.iter().all(|c| c.run_queue_len <= 5));
}

#[test]
fn pinned_task_stays_on_cpu() {
    set_cpu_count(2);
    let mut s = SmpScheduler::new(2, &[100, 101]);
    s.add_task_with_affinity(task(1, 5), AffinityMask::pinned(CpuId(1)));
    s.add_task_with_affinity(task(2, 5), AffinityMask::pinned(CpuId(1)));
    s.add_task_with_affinity(task(3, 5), AffinityMask::pinned(CpuId(0)));
    // Run a balance pass; pinned tasks must not move.
    s.balance_load();
    let snap = s.snapshot();
    assert_eq!(snap[0].run_queue_len, 1);
    assert_eq!(snap[1].run_queue_len, 2);
}

#[test]
fn migrate_changes_cpu() {
    set_cpu_count(4);
    let mut s = SmpScheduler::new(4, &[100, 101, 102, 103]);
    s.add_task(task(1, 5));
    // Initial placement on least-loaded CPU (cpu0).
    let initial = s.snapshot().iter().position(|c| c.run_queue_len > 0).unwrap();
    s.migrate(1, CpuId(3)).unwrap();
    let snap = s.snapshot();
    // The task should now be on cpu3's queue.
    assert_eq!(snap[3].run_queue_len, 1);
    // ...and no longer on the original.
    assert_eq!(snap[initial].run_queue_len, 0);
    // The scheduler should record an in/out migration.
    assert_eq!(snap[3].migrations_in, 1);
    assert_eq!(snap[initial].migrations_out, 1);
}

#[test]
fn migrate_to_invalid_cpu_errors() {
    set_cpu_count(2);
    let mut s = SmpScheduler::new(2, &[100, 101]);
    s.add_task(task(1, 5));
    let res = s.migrate(1, CpuId(99));
    assert!(res.is_err());
}

#[test]
fn task_count_matches_added_tasks() {
    set_cpu_count(2);
    let mut s = SmpScheduler::new(2, &[100, 101]);
    for i in 0..7 {
        s.add_task(task(i + 1, 5));
    }
    assert_eq!(s.task_count(), 7);
}

#[test]
fn balance_loop_is_idempotent() {
    set_cpu_count(2);
    let mut s = SmpScheduler::new(2, &[100, 101]);
    s.add_task(task(1, 5));
    s.add_task(task(2, 5));
    s.add_task(task(3, 5));
    s.balance_load();
    let total = s.task_count();
    for _ in 0..5 {
        s.balance_load();
    }
    assert_eq!(s.task_count(), total);
}