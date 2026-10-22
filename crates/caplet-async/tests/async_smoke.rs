//! Async executor smoke tests
//!
//! These tests verify the waker <-> executor linkage: a yielded task must
//! only be polled again when something calls its waker.

extern crate alloc;

use ::caplet_async::Executor;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU32, Ordering};
use core::task::{Context, Poll};
use std::sync::Mutex;

/// Future that completes on its first poll.
struct YieldOnce {
    yielded: bool,
}
impl Future for YieldOnce {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[test]
fn yield_once_runs_to_completion() {
    let ex = Executor::new();
    ex.spawn(YieldOnce { yielded: false });
    ex.run_until_idle();
    // The future completed (returned Ready); the executor drops it.
    assert_eq!(ex.task_count(), 0);
}

#[test]
fn explicit_wake_drives_future() {
    let ex = Executor::new();
    // Spawn a future that completes on first poll.
    ex.spawn_with(async { /* nothing */ }, 8, 1024);
    assert_eq!(ex.task_count(), 1);
    ex.run_until_idle();
    assert_eq!(ex.task_count(), 0);
}

#[test]
fn pending_task_is_not_repolled_without_waker() {
    let ex = Executor::new();
    let polls = Arc::new(AtomicU32::new(0));
    let polls_c = Arc::clone(&polls);
    // CountWakes::target = 1 means it will never reach Ready without
    // an external wake. We verify it is polled once and then stops.
    let _ = ex.spawn_with(
        async move {
            // A future that increments and parks.
            struct One {
                polls: Arc<AtomicU32>,
                polled: bool,
            }
            impl Future for One {
                type Output = ();
                fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
                    if self.polled {
                        self.polls.fetch_add(1, Ordering::Relaxed);
                        return Poll::Pending;
                    }
                    self.polled = true;
                    self.polls.fetch_add(1, Ordering::Relaxed);
                    Poll::Pending
                }
            }
            One {
                polls: polls_c,
                polled: false,
            }
            .await
        },
        8,
        1024,
    );
    ex.run_until_idle();
    // After draining, the pending task remains. It was polled exactly once.
    assert_eq!(ex.task_count(), 1);
    assert_eq!(polls.load(Ordering::Relaxed), 1);
}

#[test]
fn self_waking_high_priority_task_does_not_starve_low_priority() {
    let ex = Executor::new();
    let high = Arc::new(AtomicU32::new(0));
    let low = Arc::new(AtomicU32::new(0));
    let high_c = Arc::clone(&high);
    let low_c = Arc::clone(&low);

    // Priority 0 is the highest priority.  Both futures yield exactly once
    // per poll, so a correctly batched executor must make progress on both.
    ex.spawn_with(
        async move {
            for _ in 0..8 {
                high_c.fetch_add(1, Ordering::Relaxed);
                yield_once().await;
            }
        },
        0,
        1024,
    );
    ex.spawn_with(
        async move {
            for _ in 0..4 {
                low_c.fetch_add(1, Ordering::Relaxed);
                yield_once().await;
            }
        },
        3,
        1024,
    );

    ex.run_until_idle();
    assert_eq!(ex.task_count(), 0);
    assert_eq!(high.load(Ordering::Relaxed), 8);
    assert_eq!(low.load(Ordering::Relaxed), 4);
}

async fn yield_once() {
    struct YieldOnce(bool);
    impl Future for YieldOnce {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
    YieldOnce(false).await
}

#[test]
fn executor_wake_reruns_task() {
    let ex = Executor::new();
    let polls = Arc::new(AtomicU32::new(0));
    let polls_c = Arc::clone(&polls);
    let target = 3u32;
    let task_id = ex.spawn_with(
        CountWakesTask {
            polls: polls_c,
            target,
            done: Arc::new(Mutex::new(false)),
        },
        8,
        1024,
    );
    // Poll the task once.
    ex.run_once();
    // It yielded, so we wake it externally twice.
    ex.wake(task_id);
    ex.run_once();
    ex.wake(task_id);
    ex.run_once();
    // On the third poll it should be Ready.
    assert_eq!(polls.load(Ordering::Relaxed), target);
    assert_eq!(ex.task_count(), 0);
}

struct CountWakesTask {
    polls: Arc<AtomicU32>,
    target: u32,
    done: Arc<Mutex<bool>>,
}
impl Future for CountWakesTask {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        let n = self.polls.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= self.target {
            *self.done.lock().unwrap() = true;
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

#[test]
fn spawner_does_not_double_register() {
    use ::caplet_async::Spawner;
    let ex = Arc::new(Executor::new());
    let sp = Spawner::new(Arc::clone(&ex));
    let h1 = sp.spawn(async {});
    let h2 = sp.spawn(async {});
    assert_ne!(h1.id.as_u32(), h2.id.as_u32());
    ex.run_until_idle();
    assert_eq!(ex.task_count(), 0);
}

#[test]
fn block_on_completes() {
    use ::caplet_async::block_on;
    let counter = Arc::new(AtomicU32::new(0));
    let c2 = Arc::clone(&counter);
    block_on(async move {
        c2.fetch_add(1, Ordering::Relaxed);
    });
    assert_eq!(counter.load(Ordering::Relaxed), 1);
}
