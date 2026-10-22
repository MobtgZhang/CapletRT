//! Async task wrapper

use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// Async task - stackless future wrapped with priority and stack info
pub struct AsyncTask {
    /// The future (boxed and pinned)
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    /// Priority of this task
    priority: u8,
    /// Stack size requirement
    stack_size: usize,
    /// Whether this task is ready to run
    ready: bool,
}

impl AsyncTask {
    /// Create a new async task
    pub fn new<F>(future: F, priority: u8, stack_size: usize) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Self {
            future: Box::pin(future),
            priority,
            stack_size,
            ready: true,
        }
    }

    /// Create a new async task from a boxed future
    pub fn from_boxed(
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
        priority: u8,
        stack_size: usize,
    ) -> Self {
        Self {
            future,
            priority,
            stack_size,
            ready: true,
        }
    }

    /// Dissolve into its components. Used by the executor when adopting the
    /// task. This consumes the `AsyncTask`.
    pub fn dissolve(self) -> (Pin<Box<dyn Future<Output = ()> + Send>>, u8, usize) {
        (self.future, self.priority, self.stack_size)
    }

    /// Poll the task
    pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        self.future.as_mut().poll(cx)
    }

    /// Get the priority
    pub fn priority(&self) -> u8 {
        self.priority
    }

    /// Get the stack size
    pub fn stack_size(&self) -> usize {
        self.stack_size
    }

    /// Check if ready
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Mark as ready
    pub fn mark_ready(&mut self) {
        self.ready = true;
    }

    /// Mark as not ready
    pub fn mark_not_ready(&mut self) {
        self.ready = false;
    }
}

impl core::fmt::Debug for AsyncTask {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AsyncTask")
            .field("priority", &self.priority)
            .field("stack_size", &self.stack_size)
            .field("ready", &self.ready)
            .finish()
    }
}
