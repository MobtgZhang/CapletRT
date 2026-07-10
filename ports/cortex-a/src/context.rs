//! Task context for Cortex-A (AArch32).
//!
//! Stack layout matches the syscall ABI used by `__caplet_switch`:
//!
//! ```text
//! higher address
//!     +-----+ <- PSP initial
//!     | xpsr| (0x10: Thumb bit set is for Cortex-M; ignored)
//!     | lr  |
//!     | sp  |
//!     | r12 |
//!     | r3  |
//!     | r2  |
//!     | r1  |
//!     | r0  |
//!     | r11 |
//!     | r10 |
//!     | r9  |
//!     | r8  |
//!     | r7  |
//!     | r6  |
//!     | r5  |
//!     | r4  |
//!     +-----+ <- PSP after build (lowest)
//! ```
//!
//! The exact layout is the kernel's contract — see
//! `__caplet_switch` in the assembly block below.

use core::mem::offset_of;

/// Saved context for a single task.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct TaskContext {
    /// Callee-saved registers r4..r11.
    pub r4: u32,
    /// r5
    pub r5: u32,
    /// r6
    pub r6: u32,
    /// r7
    pub r7: u32,
    /// r8
    pub r8: u32,
    /// r9
    pub r9: u32,
    /// r10
    pub r10: u32,
    /// r11
    pub r11: u32,
    /// r0 — first argument / return value.
    pub r0: u32,
    /// r1
    pub r1: u32,
    /// r2
    pub r2: u32,
    /// r3
    pub r3: u32,
    /// r12
    pub r12: u32,
    /// sp — task's stack pointer.
    pub sp: u32,
    /// lr — link register to resume at.
    pub lr: u32,
    /// cpsr — saved program status register.
    pub cpsr: u32,
}

impl TaskContext {
    /// Construct an initial context.
    pub fn new(entry: u32, sp: u32, arg: u32) -> Self {
        Self {
            r4: 0,
            r5: 0,
            r6: 0,
            r7: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r0: arg,
            r1: 0,
            r2: 0,
            r3: 0,
            r12: 0,
            sp,
            lr: entry,
            // SVC mode, IRQs + FIQs disabled, ARM (not Thumb).
            cpsr: 0x13 | (1 << 7),
        }
    }

    /// Get the offset of the sp field. Used by the asm shim.
    pub const fn sp_offset() -> usize {
        offset_of!(Self, sp)
    }
}
