//! Task context for Cortex-A (AArch32).
//!
//! The cooperative switch frame is a fixed 16-word block. The kernel
//! passes the frame address in the `next` argument (the same convention
//! as the RISC-V and AArch64 ports); the first switch into a freshly
//! built frame therefore enters the task entry point directly.
//!
//! ```text
//! +0   r4      +16  r8      +32  r0      +48  r12
//! +4   r5      +20  r9      +36  r1      +52  sp
//! +8   r6      +24  r10     +40  r2      +56  lr
//! +12  r7      +28  r11     +44  r3      +60  cpsr
//! ```

use core::mem::offset_of;

/// Number of 32-bit words in the saved A32 frame.
pub const CONTEXT_WORDS: usize = 16;

/// Saved context for a single task.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct TaskContext {
    /// Callee-saved register r4.
    pub r4: u32,
    /// Callee-saved register r5.
    pub r5: u32,
    /// Callee-saved register r6.
    pub r6: u32,
    /// Callee-saved register r7.
    pub r7: u32,
    /// Callee-saved register r8.
    pub r8: u32,
    /// Callee-saved register r9.
    pub r9: u32,
    /// Callee-saved register r10.
    pub r10: u32,
    /// Callee-saved register r11.
    pub r11: u32,
    /// First argument / return value.
    pub r0: u32,
    /// Caller-saved register r1.
    pub r1: u32,
    /// Caller-saved register r2.
    pub r2: u32,
    /// Caller-saved register r3.
    pub r3: u32,
    /// Caller-saved register r12.
    pub r12: u32,
    /// Task stack pointer restored by the switch.
    pub sp: u32,
    /// Resume address / link register.
    pub lr: u32,
    /// Saved CPSR. Cooperative switches stay in SVC mode; the field is
    /// retained for diagnostics and for exception-based preemption.
    pub cpsr: u32,
}

impl TaskContext {
    /// Construct the initial context for a task.
    pub const fn new(entry: u32, sp: u32, arg: u32) -> Self {
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
            // SVC mode, IRQs/FIQs masked, ARM state.
            cpsr: 0x13 | (1 << 7) | (1 << 6),
        }
    }

    /// Offset of the saved stack-pointer field.
    pub const fn sp_offset() -> usize {
        offset_of!(Self, sp)
    }
}

/// Build an initial context at the top of `stack_top`.
///
/// The returned pointer is the frame address that the kernel stores in
/// `ScheduledTask::context`.
pub fn build_initial_stack(stack_top: *mut u32, entry: u32, arg: u32) -> *mut u32 {
    // Do the frame arithmetic with `usize` so the function is also safe
    // to exercise from a 64-bit host unit test.  Only the value stored in
    // the target's `sp` field is narrowed; dereferencing the narrowed
    // value would be invalid on the host.
    let final_sp = stack_top as usize & !7usize;
    let frame_bytes = (CONTEXT_WORDS * core::mem::size_of::<u32>()) as u32;
    let base = final_sp.wrapping_sub(frame_bytes as usize) & !7usize;
    let ctx = TaskContext::new(entry, final_sp as u32, arg);
    // SAFETY: callers provide a writable task stack large enough for
    // the 64-byte frame below `stack_top`.
    unsafe { core::ptr::write_volatile(base as *mut TaskContext, ctx) };
    base as *mut u32
}

#[cfg(all(target_arch = "arm", not(test)))]
core::arch::global_asm!(
    r#"
    .syntax unified
    .arm

    .section .text.__caplet_switch, "ax"
    .global __caplet_switch
    .type __caplet_switch, %function
__caplet_switch:
    sub     sp, sp, #64
    stmia   sp, {{r4-r11}}
    str     lr, [sp, #56]
    mrs     r3, cpsr
    str     r3, [sp, #60]
    /* Publish the outgoing frame. */
    str     sp, [r0]
    /* The value in the saved sp field is the stack pointer to restore,
     * not the frame address. */
    add     r3, sp, #64
    str     r3, [sp, #52]
    /* `next` is already the frame address, exactly like the RISC-V and
     * AArch64 ports. */
    mov     r2, r1
    ldmia   r2, {{r4-r11}}
    ldr     r0, [r2, #32]
    ldr     r12, [r2, #48]
    ldr     lr, [r2, #56]
    ldr     sp, [r2, #52]
    bx      lr
    .size __caplet_switch, . - __caplet_switch

    .section .text.__caplet_start_first, "ax"
    .global __caplet_start_first
    .type __caplet_start_first, %function
__caplet_start_first:
    mov     r1, r0
    ldmia   r1!, {{r4-r11}}
    ldr     r0, [r1, #0]
    ldr     r12, [r1, #16]
    ldr     lr, [r1, #24]
    ldr     sp, [r1, #20]
    bx      lr
    .size __caplet_start_first, . - __caplet_start_first
    "#
);

#[cfg(target_arch = "arm")]
unsafe extern "C" {
    /// Save the current context into `*prev` and resume `next`.
    pub fn __caplet_switch(prev: *mut usize, next: *const usize);
    /// Enter the first task; never returns.
    pub fn __caplet_start_first(next: *const usize) -> !;
}

#[cfg(not(target_arch = "arm"))]
/// Host stub: records nothing and returns immediately.
///
/// # Safety
/// The pointers are ignored on the host.
pub unsafe extern "C" fn __caplet_switch(_prev: *mut usize, _next: *const usize) {}

#[cfg(not(target_arch = "arm"))]
/// Host stub for the first-task entry point.
///
/// # Safety
/// Never returns; the pointer is ignored.
pub unsafe extern "C" fn __caplet_start_first(_next: *const usize) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_layout_is_stable() {
        assert_eq!(core::mem::size_of::<TaskContext>(), CONTEXT_WORDS * 4);
        assert_eq!(offset_of!(TaskContext, r0), 32);
        assert_eq!(offset_of!(TaskContext, r12), 48);
        assert_eq!(offset_of!(TaskContext, sp), 52);
        assert_eq!(offset_of!(TaskContext, lr), 56);
        assert_eq!(offset_of!(TaskContext, cpsr), 60);
        assert_eq!(TaskContext::sp_offset(), 52);
    }

    #[test]
    fn initial_context_uses_entry_and_argument() {
        let mut stack = [0u32; 64];
        let top = unsafe { stack.as_mut_ptr().add(stack.len()) };
        let frame = build_initial_stack(top, 0x6000_1234, 7) as *const TaskContext;
        let ctx = unsafe { &*frame };
        assert_eq!(ctx.lr, 0x6000_1234);
        assert_eq!(ctx.r0, 7);
        assert_eq!(ctx.sp, top as usize as u32);
        assert_eq!(ctx.sp & 7, 0);
    }
}
