//! Block on executor

use crate::executor::Executor;
use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;

/// Block the current thread on a future.
///
/// Spawns the future as the only task in a fresh executor, then runs it to
/// completion.
pub fn block_on<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    let executor = Executor::new();
    executor.spawn_with(future, 16, 2048);
    executor.run();
}

/// Block on a boxed future.
pub fn block_on_boxed(future: Pin<Box<dyn Future<Output = ()> + Send>>) {
    let executor = Executor::new();
    executor.spawn_async_task(crate::async_task::AsyncTask::from_boxed(future, 16, 2048));
    executor.run();
}
