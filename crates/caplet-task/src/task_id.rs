//! Task ID generation

use core::sync::atomic::{AtomicU32, Ordering};

/// Task ID type
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(u32);

impl TaskId {
    /// Create a new task ID
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the raw ID value
    pub fn as_u32(&self) -> u32 {
        self.0
    }

    /// Get the index part of the ID
    pub fn index(&self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    /// Get the generation part of the ID
    pub fn generation(&self) -> u16 {
        ((self.0 >> 16) & 0xFFFF) as u16
    }

    /// Check if this is the idle task
    pub fn is_idle(&self) -> bool {
        self.0 == 0
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new(0)
    }
}

impl core::fmt::Display for TaskId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Task({})", self.0)
    }
}

/// Task ID allocator
pub struct TaskIdAllocator {
    next_id: AtomicU32,
}

impl TaskIdAllocator {
    /// Create a new allocator
    pub fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1), // Start from 1, 0 is reserved for idle
        }
    }

    /// Allocate a new task ID
    pub fn allocate(&self) -> TaskId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        TaskId::new(id)
    }

    /// Get the next ID without allocating
    pub fn peek(&self) -> TaskId {
        TaskId::new(self.next_id.load(Ordering::Relaxed))
    }
}

impl Default for TaskIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_id_parts() {
        let id = TaskId::new(0x0001_0001);
        assert_eq!(id.index(), 1);
        assert_eq!(id.generation(), 1);
    }

    #[test]
    fn test_idle_task() {
        let idle = TaskId::new(0);
        assert!(idle.is_idle());
    }
}
