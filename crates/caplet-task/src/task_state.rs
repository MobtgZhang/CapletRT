//! Task state management

/// Task priority (0-31, lower is higher priority)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskPriority(u8);

impl TaskPriority {
    /// Create a new priority (0 = highest, 31 = lowest)
    pub fn new(priority: u8) -> Self {
        Self(priority.min(31))
    }

    /// Get the raw priority value
    pub fn as_u8(&self) -> u8 {
        self.0
    }

    /// Highest priority (0)
    pub fn highest() -> Self {
        Self(0)
    }

    /// Lowest priority (31)
    pub fn lowest() -> Self {
        Self(31)
    }

    /// Default priority for user tasks
    pub fn default_user() -> Self {
        Self(16)
    }

    /// Check if this is higher priority than another
    pub fn is_higher_than(&self, other: &TaskPriority) -> bool {
        self.0 < other.0
    }

    /// Check if this is lower priority than another
    pub fn is_lower_than(&self, other: &TaskPriority) -> bool {
        self.0 > other.0
    }
}

impl Default for TaskPriority {
    fn default() -> Self {
        Self::default_user()
    }
}

impl From<u8> for TaskPriority {
    fn from(p: u8) -> Self {
        Self::new(p)
    }
}

impl From<TaskPriority> for u8 {
    fn from(p: TaskPriority) -> Self {
        p.0
    }
}

/// Task state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaskState {
    /// Task is created but not yet started
    #[default]
    Created,
    /// Task is ready to run
    Ready,
    /// Task is currently running
    Running,
    /// Task is blocked waiting for a resource
    Blocked,
    /// Task is waiting for an event
    Waiting,
    /// Task has terminated
    Terminated,
    /// Task is suspended
    Suspended,
}

impl TaskState {
    /// Check if task can be scheduled
    pub fn is_schedulable(&self) -> bool {
        matches!(self, TaskState::Ready | TaskState::Running)
    }

    /// Check if task is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, TaskState::Terminated)
    }

    /// Check if task is alive (not terminated)
    pub fn is_alive(&self) -> bool {
        !self.is_terminal()
    }

    /// Check if task is blocked
    pub fn is_blocked(&self) -> bool {
        matches!(self, TaskState::Blocked | TaskState::Waiting)
    }
}

/// Transition reason for state changes
#[derive(Debug, Clone, Copy)]
pub enum StateTransitionReason {
    /// Normal scheduling
    Scheduling,
    /// Resource acquired
    ResourceAcquired,
    /// Resource released
    ResourceReleased,
    /// Event occurred
    EventOccurred,
    /// Timeout expired
    TimeoutExpired,
    /// Task completed
    Completed,
    /// Task failed
    Failed,
    /// Explicit suspend
    Suspend,
    /// Explicit resume
    Resume,
}

/// State transition result
#[derive(Debug)]
pub struct StateTransition {
    /// Previous state
    pub from: TaskState,
    /// New state
    pub to: TaskState,
    /// Reason for the transition
    pub reason: StateTransitionReason,
}

impl StateTransition {
    /// Create a new state transition
    pub fn new(from: TaskState, to: TaskState, reason: StateTransitionReason) -> Self {
        Self { from, to, reason }
    }
}
