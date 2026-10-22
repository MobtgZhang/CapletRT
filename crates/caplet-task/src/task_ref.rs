//! Task reference types

use crate::task_id::TaskId;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU8, Ordering};

/// Reference to a task (can be shared across threads)
pub struct TaskRef {
    /// Task ID
    id: TaskId,
    /// Reference count
    ref_count: Arc<AtomicU8>,
}

impl TaskRef {
    /// Create a new task reference
    pub fn new(id: TaskId) -> Self {
        Self {
            id,
            ref_count: Arc::new(AtomicU8::new(1)),
        }
    }

    /// Create a weak reference
    pub fn as_weak(&self) -> WeakTaskRef {
        WeakTaskRef { id: self.id }
    }

    /// Get the task ID
    pub fn id(&self) -> TaskId {
        self.id
    }
}

impl Clone for TaskRef {
    fn clone(&self) -> Self {
        self.ref_count.fetch_add(1, Ordering::Relaxed);
        Self {
            id: self.id,
            ref_count: Arc::clone(&self.ref_count),
        }
    }
}

impl core::fmt::Debug for TaskRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TaskRef")
            .field("id", &self.id)
            .field("ref_count", &self.ref_count.load(Ordering::Relaxed))
            .finish()
    }
}

/// Weak reference to a task
#[derive(Debug, Clone)]
pub struct WeakTaskRef {
    /// Task ID
    id: TaskId,
}

impl WeakTaskRef {
    /// Create a new weak reference
    pub fn new(id: TaskId) -> Self {
        Self { id }
    }

    /// Get the task ID
    pub fn id(&self) -> TaskId {
        self.id
    }

    /// Try to upgrade to a strong reference
    pub fn upgrade(&self) -> Option<TaskRef> {
        Some(TaskRef::new(self.id))
    }
}
