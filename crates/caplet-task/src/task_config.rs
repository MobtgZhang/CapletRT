//! Task configuration

use crate::task_state::TaskPriority;

/// Configuration for task creation
#[derive(Debug, Clone, Copy)]
pub struct TaskConfig {
    /// Task priority (0-31)
    priority: TaskPriority,
    /// Stack size in bytes
    stack_size: usize,
    /// Time slice in ticks (0 = no time slicing)
    time_slice: u32,
    /// Whether task can be preempted
    preemptible: bool,
    /// Whether task starts immediately
    autostart: bool,
    /// CPU affinity mask (for multi-core)
    cpu_affinity: u8,
    /// Heap memory budget in bytes.
    memory_budget: usize,
    /// Message quota per one-second epoch.
    message_quota: u32,
    /// CPU time quota per epoch, in microseconds.
    cpu_quota_per_epoch: u32,
}

impl TaskConfig {
    /// Create a new configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set task priority
    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set stack size
    pub fn with_stack_size(mut self, size: usize) -> Self {
        self.stack_size = size;
        self
    }

    /// Set time slice (for round-robin at same priority)
    pub fn with_time_slice(mut self, ticks: u32) -> Self {
        self.time_slice = ticks;
        self
    }

    /// Make task preemptible
    pub fn with_preemption(mut self, preemptible: bool) -> Self {
        self.preemptible = preemptible;
        self
    }

    /// Set autostart behavior
    pub fn with_autostart(mut self, autostart: bool) -> Self {
        self.autostart = autostart;
        self
    }

    /// Set CPU affinity
    pub fn with_cpu_affinity(mut self, cpu: u8) -> Self {
        self.cpu_affinity = cpu;
        self
    }

    /// Set the heap memory budget in bytes.
    pub fn with_memory_budget(mut self, bytes: usize) -> Self {
        self.memory_budget = bytes;
        self
    }

    /// Set the message quota per one-second epoch.
    pub fn with_message_quota(mut self, messages: u32) -> Self {
        self.message_quota = messages;
        self
    }

    /// Set the CPU-time quota per epoch in microseconds.
    pub fn with_cpu_quota(mut self, micros: u32) -> Self {
        self.cpu_quota_per_epoch = micros;
        self
    }

    /// Get priority
    pub fn priority(&self) -> TaskPriority {
        self.priority
    }

    /// Get stack size
    pub fn stack_size(&self) -> usize {
        self.stack_size
    }

    /// Get time slice
    pub fn time_slice(&self) -> u32 {
        self.time_slice
    }

    /// Check if preemptible
    pub fn is_preemptible(&self) -> bool {
        self.preemptible
    }

    /// Check if autostart
    pub fn autostart(&self) -> bool {
        self.autostart
    }

    /// Get CPU affinity
    pub fn cpu_affinity(&self) -> u8 {
        self.cpu_affinity
    }

    /// Get the heap memory budget in bytes.
    pub fn memory_budget(&self) -> usize {
        self.memory_budget
    }

    /// Get the message quota per one-second epoch.
    pub fn message_quota(&self) -> u32 {
        self.message_quota
    }

    /// Get the CPU-time quota per epoch in microseconds.
    pub fn cpu_quota(&self) -> u32 {
        self.cpu_quota_per_epoch
    }
}

impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            priority: TaskPriority::default_user(),
            stack_size: 2048,
            time_slice: 0, // No time slicing by default
            preemptible: true,
            autostart: true,
            cpu_affinity: 0,
            memory_budget: 1024,
            message_quota: 100,
            cpu_quota_per_epoch: 10_000,
        }
    }
}

/// Builder for TaskConfig
pub struct TaskConfigBuilder {
    config: TaskConfig,
}

impl TaskConfigBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: TaskConfig::new(),
        }
    }

    /// Build the configuration
    pub fn build(self) -> TaskConfig {
        self.config
    }

    /// Set priority
    pub fn priority(mut self, priority: u8) -> Self {
        self.config = self.config.with_priority(TaskPriority::new(priority));
        self
    }

    /// Set stack size
    pub fn stack_size(mut self, size: usize) -> Self {
        self.config = self.config.with_stack_size(size);
        self
    }

    /// Set time slice
    pub fn time_slice(mut self, ticks: u32) -> Self {
        self.config = self.config.with_time_slice(ticks);
        self
    }

    /// Set preemptible
    pub fn preemptible(mut self, value: bool) -> Self {
        self.config = self.config.with_preemption(value);
        self
    }

    /// Set autostart
    pub fn autostart(mut self, value: bool) -> Self {
        self.config = self.config.with_autostart(value);
        self
    }

    /// Set the heap memory budget in bytes.
    pub fn memory_budget(mut self, bytes: usize) -> Self {
        self.config = self.config.with_memory_budget(bytes);
        self
    }

    /// Set the message quota per one-second epoch.
    pub fn message_quota(mut self, messages: u32) -> Self {
        self.config = self.config.with_message_quota(messages);
        self
    }

    /// Set the CPU-time quota per epoch in microseconds.
    pub fn cpu_quota(mut self, micros: u32) -> Self {
        self.config = self.config.with_cpu_quota(micros);
        self
    }
}

impl Default for TaskConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_defaults_match_budget_defaults() {
        let cfg = TaskConfig::default();
        assert_eq!(cfg.memory_budget(), 1024);
        assert_eq!(cfg.message_quota(), 100);
        assert_eq!(cfg.cpu_quota(), 10_000);
    }

    #[test]
    fn builders_forward_quota_limits() {
        let cfg = TaskConfigBuilder::new()
            .memory_budget(4096)
            .message_quota(32)
            .cpu_quota(2500)
            .build();
        assert_eq!(cfg.memory_budget(), 4096);
        assert_eq!(cfg.message_quota(), 32);
        assert_eq!(cfg.cpu_quota(), 2500);
    }
}
