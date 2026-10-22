//! Ready queue implementation

use crate::priority::Priority;
use crate::scheduler::ScheduledTask;
use alloc::collections::BinaryHeap;

/// Ready queue - holds tasks that are ready to run
pub struct ReadyQueue {
    /// Priority-ordered queue of ready tasks
    queue: BinaryHeap<ScheduledTask>,
    /// Bitmap for fast priority lookup
    priority_bitmap: u32,
}

impl ReadyQueue {
    /// Create a new ready queue
    pub const fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            priority_bitmap: 0,
        }
    }

    /// Add a task to the ready queue
    pub fn enqueue(&mut self, task: ScheduledTask) {
        let priority = task.priority;
        self.priority_bitmap |= 1u32 << (31 - priority.0);
        self.queue.push(task);
    }

    /// Remove and return the highest priority task
    pub fn dequeue(&mut self) -> Option<ScheduledTask> {
        let task = self.queue.pop();
        if task.is_some() {
            self.rebuild_bitmap();
        }
        task
    }

    /// Peek at the highest priority task
    pub fn peek(&self) -> Option<&ScheduledTask> {
        self.queue.peek()
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Get the number of tasks in the queue
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Remove a specific task by ID
    pub fn remove(&mut self, task_id: u32) -> Option<ScheduledTask> {
        // BinaryHeap doesn't support removal, so we need to rebuild
        let mut found: Option<ScheduledTask> = None;
        let mut new_queue = BinaryHeap::new();

        while let Some(task) = self.queue.pop() {
            if task.id == task_id {
                found = Some(task);
            } else {
                new_queue.push(task);
            }
        }

        self.queue = new_queue;
        self.rebuild_bitmap();
        found
    }

    /// Clear the queue
    pub fn clear(&mut self) {
        self.queue.clear();
        self.priority_bitmap = 0;
    }

    /// Rebuild the priority bitmap
    fn rebuild_bitmap(&mut self) {
        self.priority_bitmap = 0;
        for task in &self.queue {
            self.priority_bitmap |= 1u32 << (31 - task.priority.0);
        }
    }

    /// Get the highest ready priority
    pub fn highest_priority(&self) -> Option<Priority> {
        if self.priority_bitmap == 0 {
            return None;
        }
        let bit_pos = self.priority_bitmap.leading_zeros();
        Some(Priority::new(31 - bit_pos as u8))
    }

    /// Snapshot the ids of the tasks currently in the queue.
    ///
    /// Provided for callers that need to inspect candidates *before*
    /// mutating the queue — notably the SMP load balancer, which must
    /// consult per-task affinity before deciding which task (if any) may
    /// be migrated. `BinaryHeap` offers no stable iteration order, so the
    /// returned order is unspecified; callers must not depend on it.
    pub fn iter_ids(&self) -> alloc::vec::Vec<u32> {
        self.queue.iter().map(|t| t.id).collect()
    }
}

impl Default for ReadyQueue {
    fn default() -> Self {
        Self::new()
    }
}
