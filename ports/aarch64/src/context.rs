//! Cooperative task context for AArch64.
//!
//! The frame preserves x19-x30, the task stack pointer and the first
//! argument x0. It is padded to 16 double-words (128 bytes), which
//! keeps SP 16-byte aligned as required by AAPCS64.

/// Number of 64-bit words in an AArch64 cooperative frame.
pub const CONTEXT_WORDS: usize = 16;

/// Saved AArch64 task context.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TaskContext {
    /// x19
    pub x19: u64,
    /// x20
    pub x20: u64,
    /// x21
    pub x21: u64,
    /// x22
    pub x22: u64,
    /// x23
    pub x23: u64,
    /// x24
    pub x24: u64,
    /// x25
    pub x25: u64,
    /// x26
    pub x26: u64,
    /// x27
    pub x27: u64,
    /// x28
    pub x28: u64,
    /// Frame pointer (x29)
    pub x29: u64,
    /// Link register / resume address (x30)
    pub x30: u64,
    /// Frame address recorded by `__caplet_switch` (the value stored
    /// through the `prev` pointer).  This is *not* the task's live SP:
    /// the live SP is `frame + 128` after restoring the frame.
    pub sp: u64,
    /// Saved ELR_EL1 (for exception-based entry).
    pub elr: u64,
    /// Saved SPSR_EL1.
    pub spsr: u64,
    /// First argument (x0).
    pub x0: u64,
}

impl TaskContext {
    /// Construct a fresh context that starts at `entry`.
    pub fn new(entry: u64, sp: u64, arg: u64) -> Self {
        Self {
            x19: 0,
            x20: 0,
            x21: 0,
            x22: 0,
            x23: 0,
            x24: 0,
            x25: 0,
            x26: 0,
            x27: 0,
            x28: 0,
            x29: 0,
            x30: entry,
            sp,
            elr: entry,
            // EL1h with DAIF masked.
            spsr: 0x3c5,
            x0: arg,
        }
    }
}

/// Build an initial context at the top of `stack_top`.
pub fn build_initial_stack(stack_top: *mut u64, entry: u64, arg: u64) -> *mut u64 {
    let frame_bytes = (CONTEXT_WORDS * core::mem::size_of::<u64>()) as u64;
    let final_sp = (stack_top as u64) & !15u64;
    let base = final_sp.saturating_sub(frame_bytes) & !15u64;
    let ctx = TaskContext::new(entry, final_sp, arg);
    // SAFETY: the caller guarantees the task stack is writable.
    unsafe { core::ptr::write_volatile(base as *mut TaskContext, ctx) };
    base as *mut u64
}

#[cfg(all(target_arch = "aarch64", not(test)))]
core::arch::global_asm!(
    r#"
    .section .text.__caplet_switch, "ax"
    .balign 4
    .global __caplet_switch
    .type __caplet_switch, %function
__caplet_switch:
    sub sp, sp, #128
    stp x19, x20, [sp, #0]
    stp x21, x22, [sp, #16]
    stp x23, x24, [sp, #32]
    stp x25, x26, [sp, #48]
    stp x27, x28, [sp, #64]
    stp x29, x30, [sp, #80]
    mov x9, sp
    /* Arch::context_switch(prev: *mut usize, next: *const usize):
     * the caller stores the *frame address* (saved SP) through `prev`,
     * exactly like the RISC-V and Cortex-M ports. */
    str x9, [x0]
    str xzr, [sp, #104]
    str xzr, [sp, #112]
    str xzr, [sp, #120]
    mov sp, x1
    ldp x19, x20, [sp, #0]
    ldp x21, x22, [sp, #16]
    ldp x23, x24, [sp, #32]
    ldp x25, x26, [sp, #48]
    ldp x27, x28, [sp, #64]
    ldp x29, x30, [sp, #80]
    ldr x0, [sp, #120]
    add sp, sp, #128
    ret
    .size __caplet_switch, . - __caplet_switch

    .section .text.__caplet_start_first, "ax"
    .balign 4
    .global __caplet_start_first
    .type __caplet_start_first, %function
__caplet_start_first:
    mov sp, x0
    ldp x19, x20, [sp, #0]
    ldp x21, x22, [sp, #16]
    ldp x23, x24, [sp, #32]
    ldp x25, x26, [sp, #48]
    ldp x27, x28, [sp, #64]
    ldp x29, x30, [sp, #80]
    /* Restore the first task's argument (x0) from the frame. */
    ldr x0, [sp, #120]
    add sp, sp, #128
    ret
    .size __caplet_start_first, . - __caplet_start_first
    "#
);

#[cfg(target_arch = "aarch64")]
extern "C" {
    /// Save the current context into `*prev` and resume `next`.
    pub fn __caplet_switch(prev: *mut usize, next: *const usize);
    /// Enter the first task; never returns.
    pub fn __caplet_start_first(next: *const usize) -> !;
}

/// Host-testing stub.  Context switching is only meaningful on a real
/// AArch64 target; host unit tests never enter the kernel scheduler.
///
/// # Safety
/// `prev` and `next` must point to valid, aligned context-word storage.
#[cfg(not(target_arch = "aarch64"))]
pub unsafe fn __caplet_switch(_prev: *mut usize, _next: *const usize) {}

/// Host-testing stub for the first-task entry point.
///
/// # Safety
/// `next` must point to a valid context constructed for this target.
#[cfg(not(target_arch = "aarch64"))]
pub unsafe fn __caplet_start_first(_next: *const usize) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout() {
        assert_eq!(
            core::mem::size_of::<TaskContext>(),
            CONTEXT_WORDS * core::mem::size_of::<u64>()
        );
    }
}
