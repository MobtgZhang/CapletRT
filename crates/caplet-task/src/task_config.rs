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
}

impl Default for TaskConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
