//! Task builder

use crate::task::Task;
use crate::task_config::TaskConfig;
use crate::task_id::TaskId;
use core::future::Future;

/// Builder for creating tasks
pub struct TaskBuilder<F: Future<Output = ()>> {
    config: TaskConfig,
    name: &'static str,
    future: Option<F>,
}

impl<F: Future<Output = ()>> TaskBuilder<F> {
    /// Create a new builder
    pub fn new(future: F, name: &'static str) -> Self {
        Self {
            config: TaskConfig::new(),
            name,
            future: Some(future),
        }
    }

    /// Set the task priority
    pub fn priority(mut self, priority: u8) -> Self {
        self.config = self.config.with_priority(priority.into());
        self
    }

    /// Set the stack size
    pub fn stack_size(mut self, size: usize) -> Self {
        self.config = self.config.with_stack_size(size);
        self
    }

    /// Set time slice
    pub fn time_slice(mut self, ticks: u32) -> Self {
        self.config = self.config.with_time_slice(ticks);
        self
    }

    /// Set whether task can be preempted
    pub fn preemptible(mut self, value: bool) -> Self {
        self.config = self.config.with_preemption(value);
        self
    }

    /// Set whether task starts immediately
    pub fn autostart(mut self, value: bool) -> Self {
        self.config = self.config.with_autostart(value);
        self
    }

    /// Set CPU affinity
    pub fn cpu_affinity(mut self, cpu: u8) -> Self {
        self.config = self.config.with_cpu_affinity(cpu);
        self
    }

    /// Build the task
    pub fn build(self, id: TaskId) -> Task<F> {
        Task::new(id, self.config, self.future.unwrap(), self.name)
    }
}