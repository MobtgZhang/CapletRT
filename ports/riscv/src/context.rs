//! Context switching and trap entry for RISC-V.
//!
//! The port supports both RV32 and RV64.  A cooperative context switch
//! only has to preserve the integer callee-saved registers (`s0`-`s11`),
//! the link register (`ra`) and the first argument register (`a0`, used
//! to pass the task argument on first entry).  The saved frame is padded
//! to a fixed 16-word size so it is 16-byte aligned on both XLENs:
//!
//! | word | RV32 offset | RV64 offset | register |
//! |------|-------------|-------------|----------|
//! | 0    | 0           | 0           | ra       |
//! | 1..12| 4..48       | 8..96       | s0..s11  |
//! | 13   | 52          | 104         | a0       |
//! | 14   | 56          | 112         | reserved |
//! | 15   | 60          | 120         | reserved |
//!
//! `__caplet_switch` uses a normal `ret`, not `sret`: it is called as an
//! ordinary function from S-mode, so `sepc` has no meaningful value at
//! that point.  Trap entry uses a separate frame and returns with
//! `sret`, which is the architecturally correct path for asynchronous
//! interrupts and exceptions.

/// Number of `usize` words in a cooperative context frame.
pub const CONTEXT_WORDS: usize = 16;

/// RISC-V task context (cooperative switch frame).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TaskContext {
    /// Resume address (`ra`).
    pub ra: usize,
    /// s0 / fp
    pub s0: usize,
    /// s1
    pub s1: usize,
    /// s2
    pub s2: usize,
    /// s3
    pub s3: usize,
    /// s4
    pub s4: usize,
    /// s5
    pub s5: usize,
    /// s6
    pub s6: usize,
    /// s7
    pub s7: usize,
    /// s8
    pub s8: usize,
    /// s9
    pub s9: usize,
    /// s10
    pub s10: usize,
    /// s11
    pub s11: usize,
    /// First argument (`a0`).
    pub a0: usize,
    /// Reserved, keeps the frame 16-byte aligned on RV64.
    pub reserved0: usize,
    /// Reserved, keeps the frame 16-byte aligned on RV64.
    pub reserved1: usize,
}

impl TaskContext {
    /// Create a fresh context that starts at `entry` with `arg` in `a0`.
    pub fn new(entry: usize, _sp: usize, arg: usize) -> Self {
        Self {
            ra: entry,
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            s5: 0,
            s6: 0,
            s7: 0,
            s8: 0,
            s9: 0,
            s10: 0,
            s11: 0,
            a0: arg,
            reserved0: 0,
            reserved1: 0,
        }
    }

    /// The saved stack pointer is implicit: it is the address returned by
    /// [`build_initial_stack`].
    pub const fn stack_pointer(&self) -> usize {
        0
    }
}

/// Build an initial context at the top of `stack_top`.
///
/// Returns the value the first switch must load into `sp`.  After the
/// restore sequence the stack pointer equals `stack_top` (aligned down
/// to 16 bytes), `a0` contains `arg`, and `ret` jumps to `entry`.
pub fn build_initial_stack(stack_top: *mut usize, entry: usize, arg: usize) -> *mut usize {
    let ctx = TaskContext::new(entry, stack_top as usize, arg);
    let frame_bytes = CONTEXT_WORDS * core::mem::size_of::<usize>();
    let base = ((stack_top as usize).saturating_sub(frame_bytes)) & !15usize;
    // SAFETY: the caller guarantees that `stack_top` points to at least
    // one task stack and that the frame below it is writable.
    unsafe {
        core::ptr::write_volatile(base as *mut TaskContext, ctx);
    }
    base as *mut usize
}

// ---------------------------------------------------------------------
// RV64 assembly
// ---------------------------------------------------------------------

#[cfg(all(target_arch = "riscv64", not(test)))]
core::arch::global_asm!(
    r#"
    .section .text.__caplet_switch, "ax"
    .global __caplet_switch
    .type __caplet_switch, @function
__caplet_switch:
    addi sp, sp, -128
    sd ra,    0(sp)
    sd s0,    8(sp)
    sd s1,   16(sp)
    sd s2,   24(sp)
    sd s3,   32(sp)
    sd s4,   40(sp)
    sd s5,   48(sp)
    sd s6,   56(sp)
    sd s7,   64(sp)
    sd s8,   72(sp)
    sd s9,   80(sp)
    sd s10,  88(sp)
    sd s11,  96(sp)
    sd a0,  104(sp)
    sd zero,112(sp)
    sd zero,120(sp)
    sd sp, 0(a0)
    mv sp, a1
    ld ra,    0(sp)
    ld s0,    8(sp)
    ld s1,   16(sp)
    ld s2,   24(sp)
    ld s3,   32(sp)
    ld s4,   40(sp)
    ld s5,   48(sp)
    ld s6,   56(sp)
    ld s7,   64(sp)
    ld s8,   72(sp)
    ld s9,   80(sp)
    ld s10,  88(sp)
    ld s11,  96(sp)
    ld a0,  104(sp)
    addi sp, sp, 128
    ret
    .size __caplet_switch, . - __caplet_switch

    .section .text.__caplet_start_first, "ax"
    .global __caplet_start_first
    .type __caplet_start_first, @function
__caplet_start_first:
    mv sp, a0
    ld ra,    0(sp)
    ld s0,    8(sp)
    ld s1,   16(sp)
    ld s2,   24(sp)
    ld s3,   32(sp)
    ld s4,   40(sp)
    ld s5,   48(sp)
    ld s6,   56(sp)
    ld s7,   64(sp)
    ld s8,   72(sp)
    ld s9,   80(sp)
    ld s10,  88(sp)
    ld s11,  96(sp)
    ld a0,  104(sp)
    addi sp, sp, 128
    ret
    .size __caplet_start_first, . - __caplet_start_first

    .section .text.__riscv_trap_vector, "ax"
    .balign 4
    .global __riscv_trap_vector
    .type __riscv_trap_vector, @function
__riscv_trap_vector:
    /* 36 words = 288 bytes (RV64), 16-byte aligned. */
    addi sp, sp, -288
    sd x1,    0(sp)
    sd x3,    8(sp)
    sd x4,   16(sp)
    sd x5,   24(sp)
    sd x6,   32(sp)
    sd x7,   40(sp)
    sd x8,   48(sp)
    sd x9,   56(sp)
    sd x10,  64(sp)
    sd x11,  72(sp)
    sd x12,  80(sp)
    sd x13,  88(sp)
    sd x14,  96(sp)
    sd x15, 104(sp)
    sd x16, 112(sp)
    sd x17, 120(sp)
    sd x18, 128(sp)
    sd x19, 136(sp)
    sd x20, 144(sp)
    sd x21, 152(sp)
    sd x22, 160(sp)
    sd x23, 168(sp)
    sd x24, 176(sp)
    sd x25, 184(sp)
    sd x26, 192(sp)
    sd x27, 200(sp)
    sd x28, 208(sp)
    sd x29, 216(sp)
    sd x30, 224(sp)
    sd x31, 232(sp)
    csrr t0, sepc
    sd t0, 240(sp)
    csrr t0, sstatus
    sd t0, 248(sp)
    addi t0, sp, 288
    sd t0, 256(sp)
    csrr a0, scause
    srli a1, a0, 63
    slli a0, a0, 1
    srli a0, a0, 1
    call riscv_trap_handler
    ld t0, 248(sp)
    csrw sstatus, t0
    ld t0, 240(sp)
    csrw sepc, t0
    ld x1,    0(sp)
    ld x3,    8(sp)
    ld x4,   16(sp)
    ld x5,   24(sp)
    ld x6,   32(sp)
    ld x7,   40(sp)
    ld x8,   48(sp)
    ld x9,   56(sp)
    ld x10,  64(sp)
    ld x11,  72(sp)
    ld x12,  80(sp)
    ld x13,  88(sp)
    ld x14,  96(sp)
    ld x15, 104(sp)
    ld x16, 112(sp)
    ld x17, 120(sp)
    ld x18, 128(sp)
    ld x19, 136(sp)
    ld x20, 144(sp)
    ld x21, 152(sp)
    ld x22, 160(sp)
    ld x23, 168(sp)
    ld x24, 176(sp)
    ld x25, 184(sp)
    ld x26, 192(sp)
    ld x27, 200(sp)
    ld x28, 208(sp)
    ld x29, 216(sp)
    ld x30, 224(sp)
    ld x31, 232(sp)
    addi sp, sp, 288
    sret
    .size __riscv_trap_vector, . - __riscv_trap_vector
    "#
);

// ---------------------------------------------------------------------
// RV32 assembly
// ---------------------------------------------------------------------

#[cfg(all(target_arch = "riscv32", not(test)))]
core::arch::global_asm!(
    r#"
    .section .text.__caplet_switch, "ax"
    .global __caplet_switch
    .type __caplet_switch, @function
__caplet_switch:
    addi sp, sp, -64
    sw ra,     0(sp)
    sw s0,     4(sp)
    sw s1,     8(sp)
    sw s2,    12(sp)
    sw s3,    16(sp)
    sw s4,    20(sp)
    sw s5,    24(sp)
    sw s6,    28(sp)
    sw s7,    32(sp)
    sw s8,    36(sp)
    sw s9,    40(sp)
    sw s10,   44(sp)
    sw s11,   48(sp)
    sw a0,    52(sp)
    sw zero,  56(sp)
    sw zero,  60(sp)
    sw sp, 0(a0)
    mv sp, a1
    lw ra,     0(sp)
    lw s0,     4(sp)
    lw s1,     8(sp)
    lw s2,    12(sp)
    lw s3,    16(sp)
    lw s4,    20(sp)
    lw s5,    24(sp)
    lw s6,    28(sp)
    lw s7,    32(sp)
    lw s8,    36(sp)
    lw s9,    40(sp)
    lw s10,   44(sp)
    lw s11,   48(sp)
    lw a0,    52(sp)
    addi sp, sp, 64
    ret
    .size __caplet_switch, . - __caplet_switch

    .section .text.__caplet_start_first, "ax"
    .global __caplet_start_first
    .type __caplet_start_first, @function
__caplet_start_first:
    mv sp, a0
    lw ra,     0(sp)
    lw s0,     4(sp)
    lw s1,     8(sp)
    lw s2,    12(sp)
    lw s3,    16(sp)
    lw s4,    20(sp)
    lw s5,    24(sp)
    lw s6,    28(sp)
    lw s7,    32(sp)
    lw s8,    36(sp)
    lw s9,    40(sp)
    lw s10,   44(sp)
    lw s11,   48(sp)
    lw a0,    52(sp)
    addi sp, sp, 64
    ret
    .size __caplet_start_first, . - __caplet_start_first

    .section .text.__riscv_trap_vector, "ax"
    .balign 4
    .global __riscv_trap_vector
    .type __riscv_trap_vector, @function
__riscv_trap_vector:
    /* 36 words = 144 bytes (RV32), 16-byte aligned. */
    addi sp, sp, -144
    sw x1,    0(sp)
    sw x3,    4(sp)
    sw x4,    8(sp)
    sw x5,   12(sp)
    sw x6,   16(sp)
    sw x7,   20(sp)
    sw x8,   24(sp)
    sw x9,   28(sp)
    sw x10,  32(sp)
    sw x11,  36(sp)
    sw x12,  40(sp)
    sw x13,  44(sp)
    sw x14,  48(sp)
    sw x15,  52(sp)
    sw x16,  56(sp)
    sw x17,  60(sp)
    sw x18,  64(sp)
    sw x19,  68(sp)
    sw x20,  72(sp)
    sw x21,  76(sp)
    sw x22,  80(sp)
    sw x23,  84(sp)
    sw x24,  88(sp)
    sw x25,  92(sp)
    sw x26,  96(sp)
    sw x27, 100(sp)
    sw x28, 104(sp)
    sw x29, 108(sp)
    sw x30, 112(sp)
    sw x31, 116(sp)
    csrr t0, sepc
    sw t0, 120(sp)
    csrr t0, sstatus
    sw t0, 124(sp)
    addi t0, sp, 144
    sw t0, 128(sp)
    csrr a0, scause
    srli a1, a0, 31
    slli a0, a0, 1
    srli a0, a0, 1
    call riscv_trap_handler
    lw t0, 124(sp)
    csrw sstatus, t0
    lw t0, 120(sp)
    csrw sepc, t0
    lw x1,    0(sp)
    lw x3,    4(sp)
    lw x4,    8(sp)
    lw x5,   12(sp)
    lw x6,   16(sp)
    lw x7,   20(sp)
    lw x8,   24(sp)
    lw x9,   28(sp)
    lw x10,  32(sp)
    lw x11,  36(sp)
    lw x12,  40(sp)
    lw x13,  44(sp)
    lw x14,  48(sp)
    lw x15,  52(sp)
    lw x16,  56(sp)
    lw x17,  60(sp)
    lw x18,  64(sp)
    lw x19,  68(sp)
    lw x20,  72(sp)
    lw x21,  76(sp)
    lw x22,  80(sp)
    lw x23,  84(sp)
    lw x24,  88(sp)
    lw x25,  92(sp)
    lw x26,  96(sp)
    lw x27, 100(sp)
    lw x28, 104(sp)
    lw x29, 108(sp)
    lw x30, 112(sp)
    lw x31, 116(sp)
    addi sp, sp, 144
    sret
    .size __riscv_trap_vector, . - __riscv_trap_vector
    "#
);

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
extern "C" {
    /// Cooperative context switch (`prev` receives the current sp).
    pub fn __caplet_switch(prev_ctx: *mut usize, next_ctx: *const usize);
    /// Enter the first task; never returns.
    pub fn __caplet_start_first(next_ctx: *const usize) -> !;
    /// Global trap entry installed in `stvec`.
    pub fn __riscv_trap_vector();
}

/// Rust half of the trap dispatcher.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[no_mangle]
extern "C" fn riscv_trap_handler(cause: usize, is_interrupt: usize) {
    const SUPERVISOR_SOFTWARE: usize = 1;
    const SUPERVISOR_TIMER: usize = 5;
    const SUPERVISOR_EXTERNAL: usize = 9;

    if is_interrupt != 0 {
        match cause {
            SUPERVISOR_EXTERNAL => {
                crate::plic::Plic::handle_pending();
            }
            SUPERVISOR_TIMER => {
                crate::clint::Clint::acknowledge_timer_irq();
            }
            SUPERVISOR_SOFTWARE => {
                // sip.SSIP is writable from S-mode.  Clearing only the
                // software-pending bit avoids disturbing timer/external
                // pending state.
                unsafe {
                    core::arch::asm!("csrc sip, {bit}", bit = const 2, options(nostack));
                }
            }
            _ => {}
        }
    } else {
        crate::uart::Uart::put_line("PANIC: unhandled RISC-V supervisor exception");
        crate::uart::Uart::put_str("scause=");
        crate::uart::Uart::put_hex(cause as u64);
        crate::uart::Uart::put_line("");
        loop {
            // SAFETY: parking the hart is safe in a fatal handler.
            unsafe { core::arch::asm!("wfi", options(nostack, preserves_flags)) };
        }
    }
}

/// Pend a supervisor software interrupt (reschedule request).
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[inline(always)]
pub fn pend_software_irq() {
    // sip.SSIP is the only `sip` bit writable by S-mode, and it is the
    // correct architectural mechanism for a self-IPI without SBI.
    unsafe {
        core::arch::asm!("csrs sip, {bit}", bit = const 2, options(nostack));
    }
}

/// Host stub.
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
#[inline(always)]
pub fn pend_software_irq() {}

/// Host-testing stub. Context switching is only meaningful on a real
/// RISC-V hart; keeping the symbol available lets the architecture
/// bridge and the rest of the workspace compile for `cargo test`.
///
/// # Safety
///
/// On real hardware both pointers must be valid initial/context stacks.
/// This host stub does not dereference them.
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub unsafe extern "C" fn __caplet_switch(_prev_ctx: *mut usize, _next_ctx: *const usize) {}

/// Host-testing stub for the first-task entry point.
///
/// # Safety
///
/// `_next_ctx` must point to a valid initial context on real hardware.
/// This host stub never dereferences it.
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub unsafe extern "C" fn __caplet_start_first(_next_ctx: *const usize) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_context_layout() {
        let mut buf = [0usize; 32];
        let stack_top = unsafe { buf.as_mut_ptr().add(buf.len()) };
        let ctx_ptr = build_initial_stack(stack_top, 0xDEAD_BEEF, 0x1234);
        let ctx = unsafe { core::ptr::read_volatile(ctx_ptr as *const TaskContext) };
        assert_eq!(ctx.ra, 0xDEAD_BEEF);
        assert_eq!(ctx.a0, 0x1234);
        assert_eq!(
            core::mem::size_of::<TaskContext>(),
            CONTEXT_WORDS * core::mem::size_of::<usize>()
        );
        assert_eq!(ctx_ptr as usize % 16, 0);
        assert_eq!(
            ctx_ptr as usize + CONTEXT_WORDS * core::mem::size_of::<usize>(),
            stack_top as usize
        );
    }
}
