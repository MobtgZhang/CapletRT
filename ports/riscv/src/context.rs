//! Context switching for RISC-V S-mode.
//!
//! The context layout is modeled after the AAPCS-RISC-V calling convention.
//! We save/restore the 12 callee-saved registers (s0-s11) plus sepc and
//! sstatus. The stack grows downward (toward lower addresses).
//!
//! RISC-V S-mode register conventions:
//!   s0-s11  : callee-saved (12 registers)
//!   sepc    : saved program counter (after trap)
//!   sstatus : saved status (SIE, SPIE, SPP, etc.)
//!   sp      : stack pointer
//!
//! For an initial task, we build a fake trap frame on the stack:
//!   sepc = entry point
//!   sstatus.SIE = 0 (interrupts initially off)
//!   s0-s11 = 0
//!   sp = stack_top

/// RISC-V task context structure.
///
/// This matches the layout pushed/popped by `__caplet_switch`:
///   Offset  0: s0
///   Offset  8: s1
///   ...
///   Offset 88: s11
///   Offset 96: sepc
///   Offset 104: sstatus
///
/// Total: 14 × 8 = 112 bytes.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TaskContext {
    /// s0
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
    /// Saved program counter (trap return address).
    pub sepc: usize,
    /// Saved sstatus CSR.
    pub sstatus: usize,
}

impl TaskContext {
    /// Create a fresh task context for a new task.
    ///
    /// `entry` — the function pointer to jump to.
    /// `sp`    — the top of the task's stack (highest address, aligned).
    /// `arg`   — a user-defined argument passed in a0.
    ///
    /// Returns the sp value to store in the scheduler's slot.
    pub fn new(entry: usize, sp: usize, _arg: usize) -> Self {
        Self {
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
            // sepc is the PC to jump to when the task first runs.
            sepc: entry,
            // sstatus:
            //   SIE = 0  (interrupts disabled on first entry)
            //   SPIE = 1 (was in user mode, so restore SIE on exit)
            //   spp = 0  (previous privilege = user — not used but set)
            // In RISC-V sstatus bit layout (RV32):
            //   bit 1  = SIE ( Supervisor Interrupt Enable)
            //   bit 5  = SPIE (SIE before trap)
            //   bit 8  = SPP (previous mode: 0=user, 1=supervisor)
            sstatus: 0x40020, // SPIE=1, SPP=0, SIE=0
        }
    }

    /// Return the stack pointer from this context.
    /// Note: the actual sp is tracked separately by the scheduler; this
    /// is a placeholder.
    pub fn stack_pointer(&self) -> usize {
        0
    }
}

/// Build the initial stack for the first task.
///
/// Stack layout (high → low addresses):
///   sstatus (8B), sepc (8B), s11 (8B) .. s0 (8B)
///   = 14 × 8 = 112 bytes total
///
/// Returns a pointer to the "context" in memory, which is the sp value
/// to store in the scheduler's slot. The first context_switch call
/// loads this sp as the new stack pointer.
pub fn build_initial_stack(stack_top: *mut usize, entry: usize, arg: usize) -> *mut usize {
    let mut ctx = TaskContext::new(entry, stack_top as usize, arg);
    // a0 is the first argument register.
    ctx.s0 = arg;

    let base = stack_top.wrapping_sub(14) as *mut TaskContext;
    unsafe {
        core::ptr::write_volatile(base, ctx);
    }
    base as *mut usize
}

/// Switch context from the task pointed to by `prev_ctx` to `next_ctx`.
///
/// `prev_ctx` points to a `*mut usize` slot where the current sp will be saved.
/// `next_ctx` points to a `*const usize` where the new sp will be loaded.
///
/// On entry, both registers hold pointer values:
///   a0 → prev slot, a1 → next slot
///
/// # Safety
/// - `prev_ctx` must point to a valid slot initialized by a previous call
///   to `build_initial_stack` or by a previous `__caplet_switch`.
/// - `next_ctx` must point to a valid context built by `build_initial_stack`.
/// - This function does not return normally (it executes sret to resume the
///   next task's execution).
#[cfg(target_arch = "riscv")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __caplet_switch(prev_ctx: *mut usize, next_ctx: *const usize) {
    unsafe {
        core::arch::asm!(
            // Push current callee-saved registers onto the current stack.
            "sd s0,  0(sp)",
            "sd s1,  8(sp)",
            "sd s2,  16(sp)",
            "sd s3,  24(sp)",
            "sd s4,  32(sp)",
            "sd s5,  40(sp)",
            "sd s6,  48(sp)",
            "sd s7,  56(sp)",
            "sd s8,  64(sp)",
            "sd s9,  72(sp)",
            "sd s10, 80(sp)",
            "sd s11, 88(sp)",
            // Save sepc (the PC to resume at).
            "csrr t0, sepc",
            "sd t0, 96(sp)",
            // Save sstatus.
            "csrr t0, sstatus",
            "sd t0, 104(sp)",
            // Store current sp to the prev slot.
            "sd sp, 0(a0)",
            // Load new sp from the next context pointer.
            "ld sp, 0(a1)",
            // Restore sstatus first (enables interrupts when appropriate).
            "ld t0, 104(sp)",
            "csrw sstatus, t0",
            // Restore sepc.
            "ld t0, 96(sp)",
            "csrw sepc, t0",
            // Restore callee-saved registers.
            "ld s0,  0(sp)",
            "ld s1,  8(sp)",
            "ld s2,  16(sp)",
            "ld s3,  24(sp)",
            "ld s4,  32(sp)",
            "ld s5,  40(sp)",
            "ld s6,  48(sp)",
            "ld s7,  56(sp)",
            "ld s8,  64(sp)",
            "ld s9,  72(sp)",
            "ld s10, 80(sp)",
            "ld s11, 88(sp)",
            // Return via sret (jumps to restored sepc).
            "sret",
            inout("a0") prev_ctx,
            inout("a1") next_ctx,
            options(noreturn)
        );
    }
}

/// Start the first task by restoring its context and jumping to its PC.
///
/// `next_ctx` points to a context built by `build_initial_stack`.
///
/// # Safety
/// - `next_ctx` must be valid and aligned.
/// - The context's sepc must be a valid, aligned instruction address.
/// - This function does not return.
#[cfg(target_arch = "riscv")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __caplet_start_first(next_ctx: *mut usize) -> ! {
    unsafe {
        core::arch::asm!(
            // Load the initial sp from the context.
            "ld sp, 0(a0)",
            // Restore sstatus (clears SIE so interrupts stay off until first wfi).
            "ld t0, 104(sp)",
            "csrw sstatus, t0",
            // Restore sepc (the entry point).
            "ld t0, 96(sp)",
            "csrw sepc, t0",
            // Restore callee-saved registers (may be zero).
            "ld s0,  0(sp)",
            "ld s1,  8(sp)",
            "ld s2,  16(sp)",
            "ld s3,  24(sp)",
            "ld s4,  32(sp)",
            "ld s5,  40(sp)",
            "ld s6,  48(sp)",
            "ld s7,  56(sp)",
            "ld s8,  64(sp)",
            "ld s9,  72(sp)",
            "ld s10, 80(sp)",
            "ld s11, 88(sp)",
            // Jump to sepc via mret (mret = machine-mode return, used as
            // S-mode return-from-trap when in S-mode).
            "mret",
            inout("a0") next_ctx => _,
            options(noreturn)
        );
    }
}

/// Pend a software interrupt via CLINT to trigger a context switch.
///
/// This is the RISC-V equivalent of `pend_pendsv()` on Cortex-M.
#[cfg(target_arch = "riscv")]
#[inline(always)]
pub fn pend_software_irq() {
    // Write 1 to the CLINT msip register for hart 0.
    // CLINT base = 0x0200_0000 on QEMU virt.
    const CLINT_MSIP: usize = 0x0200_0000;
    unsafe {
        core::arch::asm!(
            "sw {val}, 0({base})",
            val = in(reg) 1u32,
            base = in(reg) CLINT_MSIP as usize,
            options(nostack, preserves_flags)
        );
    }
}

/// Pend a software interrupt (host stub).
#[cfg(not(target_arch = "riscv"))]
#[inline(always)]
pub fn pend_software_irq() {}

/// RISC-V trap vector handler.
///
/// This is the global trap entry point referenced by stvec.
/// All synchronous and asynchronous traps (exceptions and interrupts)
/// land here. A minimal implementation dispatches to appropriate handlers.
///
/// # Safety
/// This function is registered as the trap vector via `csrw stvec, addr`.
/// It must be `#[no_mangle]` and aligned to a 4-byte boundary.
#[cfg(target_arch = "riscv")]
#[unsafe(no_mangle)]
#[naked]
pub extern "C" fn __riscv_trap_vector() {
    unsafe {
        core::arch::asm!(
            // Save the current context onto the stack.
            // We need space for: s0-s11 (12×8) + sepc (8) + sstatus (8) = 120 bytes.
            "addi sp, sp, -128",
            "sd s0,  0(sp)",
            "sd s1,  8(sp)",
            "sd s2,  16(sp)",
            "sd s3,  24(sp)",
            "sd s4,  32(sp)",
            "sd s5,  40(sp)",
            "sd s6,  48(sp)",
            "sd s7,  56(sp)",
            "sd s8,  64(sp)",
            "sd s9,  72(sp)",
            "sd s10, 80(sp)",
            "sd s11, 88(sp)",
            "csrr t0, sepc",
            "sd t0, 96(sp)",
            "csrr t0, sstatus",
            "sd t0, 104(sp)",
            // Read the trap cause.
            "csrr a0, scause",
            // Distinguish: a0 >> 63 == 0 → exception, == 1 → interrupt.
            // On QEMU virt, the only asynchronous traps we expect are:
            //   1 (user software)    = CLINT msip  (SSI, bit 63 set)
            //   5 (supervisor timer)  = CLINT mtimecmp  (STI, bit 63 set)
            //   9 (supervisor ext)    = PLIC external (SEI, bit 63 set)
            "bge a0, zero, 1f",   // exception: skip interrupt path
            // --- interrupt path ---
            "srli a1, a0, 63",     // a1 = 1 if interrupt
            "andi a0, a0, 0xFFF",  // a0 = cause without interrupt bit
            "j 2f",
            // --- exception path ---
            "1:",
            "li a1, 0",
            // a0 = cause, a1 = is_interrupt
            "2:",
            // Call the Rust trap handler with (cause, is_interrupt).
            "call {handler}",
            // After handling, restore context and return via sret.
            "ld t0, 104(sp)",
            "csrw sstatus, t0",
            "ld t0, 96(sp)",
            "csrw sepc, t0",
            "ld s0,  0(sp)",
            "ld s1,  8(sp)",
            "ld s2,  16(sp)",
            "ld s3,  24(sp)",
            "ld s4,  32(sp)",
            "ld s5,  40(sp)",
            "ld s6,  48(sp)",
            "ld s7,  56(sp)",
            "ld s8,  64(sp)",
            "ld s9,  72(sp)",
            "ld s10, 80(sp)",
            "ld s11, 88(sp)",
            "addi sp, sp, 128",
            "sret",
            handler = sym riscv_trap_handler,
            options(noreturn)
        );
    }
}

/// Rust trap handler — called from the assembly vector with
/// `(cause: usize, is_interrupt: usize)` in a0/a1.
#[cfg(target_arch = "riscv")]
fn riscv_trap_handler(cause: usize, is_interrupt: usize) {
    const INTERRUPT_BIT: usize = 1 << 63;

    // 0xFFFFFFFF80000007 = Supervisor external interrupt (PLIC).
    // 0x8000000000000005 = Supervisor timer interrupt (CLINT mtimecmp).
    // 0x8000000000000001 = Supervisor software interrupt (CLINT msip).
    match cause {
        x if x == INTERRUPT_BIT | 9 => {
            // External interrupt from PLIC — UART, network, etc.
            crate::plic::Plic::acknowledge();
            crate::plic::Plic::handle_pending();
        }
        x if x == INTERRUPT_BIT | 5 => {
            // Timer interrupt — tick.
            // Clear by advancing mtimecmp past mtime.
            crate::clint::Clint::acknowledge_timer_irq();
        }
        x if x == INTERRUPT_BIT | 1 => {
            // Software interrupt — scheduler request.
            // The kernel's scheduler pended this via request_reschedule().
            crate::clint::Clint::clear_software_irq(0);
        }
        _ => {
            // Unexpected trap — halt.
            #[cfg(target_arch = "riscv")]
            {
                core::arch::asm!("ebreak", options(nostack));
            }
        }
    }

    // If this was an interrupt and a higher-priority task is now ready,
    // trigger a context switch.
    if is_interrupt == 1 {
        caplet_kernel::arch::Arch::request_reschedule();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that build_initial_stack produces a correctly laid-out
    /// context that matches the assembly's expectations.
    #[test]
    fn initial_context_layout() {
        let mut buf = [0usize; 32];
        let stack_top = buf.as_mut_ptr().wrapping_add(buf.len());
        let ctx_ptr = build_initial_stack(stack_top, 0xDEAD_BEEF, 0x1234);
        let ctx = unsafe { core::ptr::read_volatile(ctx_ptr as *const TaskContext) };
        assert_eq!(ctx.sepc, 0xDEAD_BEEF);
        assert_eq!(ctx.sstatus & 0x2, 0); // SIE = 0
        assert_eq!(ctx.sstatus & 0x40000, 0x40000); // SPIE = 1
    }
}
