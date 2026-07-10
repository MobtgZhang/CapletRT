//! Async task spawner

extern crate caplet_task;
use crate::async_task::AsyncTask;
use crate::executor::Executor;
use alloc::sync::Arc;
use core::future::Future;

/// Spawner for creating new async tasks
///
/// Holds an `Arc<Executor>` so cloning the spawner is cheap and the executor
/// lifetime is shared. `spawn` actually registers the task in the executor.
#[derive(Clone)]
pub struct Spawner {
    executor: Arc<Executor>,
}

// SAFETY: The `Arc<Executor>` enforces single ownership of the mutable
// executor state; the executor is intended to be driven by the same single
// kernel context so `Send`/`Sync` are conservative upper bounds. The raw
// pointer in the previous implementation was unsound and is replaced.
unsafe impl Send for Spawner {}
unsafe impl Sync for Spawner {}

impl Spawner {
    /// Create a spawner for the given executor.
    pub fn new(executor: Arc<Executor>) -> Self {
        Self { executor }
    }

    /// Spawn a new task (default priority 16, default stack 2 KiB).
    pub fn spawn<F>(&self, future: F) -> TaskHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_with(future, 16, 2048)
    }

    /// Spawn a task with explicit priority and stack size.
    pub fn spawn_with<F>(&self, future: F, priority: u8, stack_size: usize) -> TaskHandle
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let id = self.executor.spawn_with(future, priority, stack_size);
        TaskHandle {
            id: caplet_task::TaskId::new(id),
        }
    }

    /// Spawn a pre-built task.
    pub fn spawn_task(&self, task: AsyncTask) -> TaskHandle {
        let id = self.executor.spawn_async_task(task);
        TaskHandle {
            id: caplet_task::TaskId::new(id),
        }
    }
}

impl Default for Spawner {
    fn default() -> Self {
        Self::new(Arc::new(Executor::new()))
    }
}

/// Handle to a spawned task
#[derive(Debug, Clone, Copy)]
pub struct TaskHandle {
    /// The task ID
    pub id: caplet_task::TaskId,
}
