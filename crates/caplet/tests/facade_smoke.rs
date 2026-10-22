//! Compile-and-run smoke test for the public `#[caplet::*]` surface.

use caplet::runtime::{drive_executor, spawn_main_with};
use core::sync::atomic::{AtomicUsize, Ordering};

static MAIN_RUNS: AtomicUsize = AtomicUsize::new(0);
static TASK_RUNS: AtomicUsize = AtomicUsize::new(0);

#[caplet::task(
    priority = 3,
    stack = 1024,
    cpu_quota = 2000,
    mem_budget = 2048,
    message_quota = 7
)]
async fn counted_task(value: usize) {
    TASK_RUNS.fetch_add(value, Ordering::SeqCst);
}

#[caplet::main]
async fn main(spawner: caplet::Spawner, caps: caplet::Caplets) {
    assert_eq!(caps.service_count(), 0);
    MAIN_RUNS.fetch_add(1, Ordering::SeqCst);
    spawner.spawn_with(counted_task(2), 3, 1024);
}

#[test]
fn facade_macros_expand_and_boot_future_runs() {
    assert_eq!(
        __CAPLET_TASK_DESC_counted_task.cpu_quota, 2000,
        "task quota is emitted into the descriptor"
    );
    assert_eq!(__CAPLET_TASK_DESC_counted_task.mem_budget, 2048);
    assert_eq!(__CAPLET_TASK_DESC_counted_task.message_quota, 7);

    spawn_main_with(|spawner, caps| async move { main(spawner, caps).await });
    drive_executor();
    drive_executor();

    assert_eq!(MAIN_RUNS.load(Ordering::SeqCst), 1);
    assert_eq!(TASK_RUNS.load(Ordering::SeqCst), 2);
}
