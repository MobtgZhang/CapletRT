//! Task definition

use crate::task_id::TaskId;
use crate::task_config::TaskConfig;
use crate::task_state::{TaskState, TaskPriority};
use crate::task_context::TaskControlBlock;
use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

/// A task in the system
pub struct Task<F: Future<Output = ()> = Pin<Box<dyn Future<Output = ()>>>> {
    /// Task control block
    tcb: TaskControlBlock,
    /// The future this task is running
    future: Pin<Box<F>>,
    /// Task name for debugging
    name: &'static str,
}

impl<F: Future<Output = ()>> Task<F> {
    /// Create a new task
    pub fn new(id: TaskId, config: TaskConfig, future: F, name: &'static str) -> Self {
        let tcb = TaskControlBlock::new(
            id,
            config.priority(),
            0, // Will be set by the scheduler
            config.stack_size(),
        );

        Self {
            tcb,
            future: Box::pin(future),
            name,
        }
    }

    /// Poll the task's future
    pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        self.future.as_mut().poll(cx)
    }

    /// Get the task ID
    pub fn id(&self) -> TaskId {
        self.tcb.id
    }

    /// Get the current state
    pub fn state(&self) -> TaskState {
        self.tcb.state
    }

    /// Set the task state
    pub fn set_state(&mut self, state: TaskState) {
        self.tcb.set_state(state);
    }

    /// Get the priority
    pub fn priority(&self) -> TaskPriority {
        self.tcb.priority
    }

    /// Get the name
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Get a reference to the TCB
    pub fn tcb(&self) -> &TaskControlBlock {
        &self.tcb
    }

    /// Get a mutable reference to the TCB
    pub fn tcb_mut(&mut self) -> &mut TaskControlBlock {
        &mut self.tcb
    }

    /// Check if task can be scheduled
    pub fn is_schedulable(&self) -> bool {
        self.tcb.state.is_schedulable()
    }

    /// Check if task is ready to run
    pub fn is_ready(&self) -> bool {
        self.tcb.state == TaskState::Ready
    }

    /// Check if task is running
    pub fn is_running(&self) -> bool {
        self.tcb.state == TaskState::Running
    }

    /// Transition to ready state
    pub fn wake(&mut self) {
        if self.tcb.state == TaskState::Blocked || self.tcb.state == TaskState::Waiting {
            self.tcb.state = TaskState::Ready;
        }
    }

    /// Transition to blocked state
    pub fn block(&mut self) {
        self.tcb.state = TaskState::Blocked;
    }

    /// Transition to waiting state
    pub fn wait(&mut self) {
        self.tcb.state = TaskState::Waiting;
    }

    /// Mark task as terminated
    pub fn terminate(&mut self) {
        self.tcb.state = TaskState::Terminated;
    }

    /// Check if task is terminated
    pub fn is_terminated(&self) -> bool {
        self.tcb.state.is_terminal()
    }

    /// Check if task is alive
    pub fn is_alive(&self) -> bool {
        self.tcb.state.is_alive()
    }
}

impl<F: Future<Output = ()>> Drop for Task<F> {
    fn drop(&mut self) {
        // Clean up task resources
        self.terminate();
    }
}
