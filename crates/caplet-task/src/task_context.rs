//! Task context management

use crate::task_id::TaskId;
use crate::task_state::{TaskState, TaskPriority};

/// Saved task context
///
/// This structure holds the minimum state needed to suspend and resume a task.
/// On Cortex-M, this maps to the stack frame layout.
#[derive(Debug)]
pub struct TaskContext {
    /// Stack pointer value
    pub sp: u32,
    /// Program counter (where to resume)
    pub pc: u32,
    /// Link register (LR)
    pub lr: u32,
    /// R0-R12 general purpose registers
    pub r: [u32; 13],
    /// Program status register
    pub xpsr: u32,
}

impl TaskContext {
    /// Create a new context for a task entry point
    pub fn new(entry: u32, sp: u32, arg: u32) -> Self {
        Self {
            sp,
            pc: entry,
            lr: 0xFFFFFFFD, // Return to thread mode, PSP
            r: [0; 13],
            xpsr: 0x01000000, // Default PSR (Thumb bit set)
        }
    }

    /// Get the stack pointer
    pub fn stack_pointer(&self) -> u32 {
        self.sp
    }

    /// Set the stack pointer
    pub fn set_stack_pointer(&mut self, sp: u32) {
        self.sp = sp;
    }

    /// Check if the context is valid (has a valid stack)
    pub fn is_valid(&self) -> bool {
        self.sp != 0 && self.pc != 0
    }
}

impl Default for TaskContext {
    fn default() -> Self {
        Self {
            sp: 0,
            pc: 0,
            lr: 0,
            r: [0; 13],
            xpsr: 0,
        }
    }
}

/// Task control block (TCB)
#[derive(Debug)]
pub struct TaskControlBlock {
    /// Task ID
    pub id: TaskId,
    /// Current state
    pub state: TaskState,
    /// Priority
    pub priority: TaskPriority,
    /// Context for context switching
    pub context: TaskContext,
    /// Remaining time slice
    pub time_slice_remaining: u32,
    /// Stack base address
    pub stack_base: u32,
    /// Stack size
    pub stack_size: usize,
    /// Stack high water mark
    pub stack_high_water: usize,
}

impl TaskControlBlock {
    /// Create a new TCB
    pub fn new(id: TaskId, priority: TaskPriority, stack_base: u32, stack_size: usize) -> Self {
        Self {
            id,
            state: TaskState::Created,
            priority,
            context: TaskContext::default(),
            time_slice_remaining: 0,
            stack_base,
            stack_size,
            stack_high_water: stack_size,
        }
    }

    /// Update the stack high water mark
    pub fn update_stack_high_water(&mut self, current_sp: u32) {
        let used = (self.stack_base as i32 - current_sp as i32) as usize;
        if used < self.stack_high_water {
            self.stack_high_water = used;
        }
    }

    /// Check for stack overflow
    pub fn check_stack_overflow(&self, current_sp: u32) -> bool {
        let used = (self.stack_base as i32 - current_sp as i32) as usize;
        used >= self.stack_size
    }

    /// Transition to a new state
    pub fn set_state(&mut self, new_state: TaskState) {
        self.state = new_state;
    }

    /// Get state
    pub fn state(&self) -> TaskState {
        self.state
    }
}
