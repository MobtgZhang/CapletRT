//! Context switching for Cortex-M
//!
//! The actual context switch is an `extern "C"` assembly routine that
//! saves the callee-saved registers of the outgoing task onto its stack
//! and restores the callee-saved registers of the incoming task.
//!
//! For full preemptive scheduling we use the PendSV exception.  The
//! hardware has already pushed the exception frame
//! (`r0-r3, r12, lr, pc, xPSR`) onto the task's *process* stack when the
//! handler runs, so `__caplet_switch` must operate on `PSP`, not on the
//! handler stack (`MSP`).  After restoring `r4-r11` it performs an
//! exception return with `EXC_RETURN = 0xFFFFFFFD`, which makes the
//! hardware unstack the new task's frame from `PSP`.

extern crate alloc;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicPtr, Ordering};

/// Task context for context switching.
///
/// This is the layout we push on the stack in software: r4-r11, r0-r3,
/// r12, lr, pc, xpsr. The first 8 are callee-saved in the AAPCS; the
/// rest are part of the hardware exception frame.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TaskContext {
    /// R4
    pub r4: u32,
    /// R5
    pub r5: u32,
    /// R6
    pub r6: u32,
    /// R7
    pub r7: u32,
    /// R8
    pub r8: u32,
    /// R9
    pub r9: u32,
    /// R10
    pub r10: u32,
    /// R11
    pub r11: u32,
    /// R0 — first argument / return value
    pub r0: u32,
    /// R1
    pub r1: u32,
    /// R2
    pub r2: u32,
    /// R3
    pub r3: u32,
    /// R12
    pub r12: u32,
    /// LR — link register restored into the task (not EXC_RETURN)
    pub lr: u32,
    /// PC — return address
    pub pc: u32,
    /// xPSR
    pub xpsr: u32,
}

impl TaskContext {
    /// Create a new context for a task. `sp` is the initial stack
    /// pointer (lowest free address — Cortex-M grows down).
    pub fn new(entry: u32, _sp: u32, arg: u32) -> Self {
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
            // This is the *thread* LR value.  EXC_RETURN is supplied by
            // the switch routine itself when it executes `bx lr`.
            lr: 0,
            pc: entry,
            xpsr: 0x0100_0000, // Thumb bit set
        }
    }

    /// Initial stack pointer (lowest free address). Currently unused —
    /// `sp` is tracked separately by the scheduler.
    pub fn stack_pointer(&self) -> u32 {
        0
    }
}

/// Switch context from `old_sp` to `new_sp`.
///
/// On entry, r0 points to the old PSP slot, r1 points to the new PSP
/// slot. The routine saves r4-r11 on the old process stack, switches
/// PSP, restores r4-r11 from the new process stack, and performs an
/// exception return into the new task.
///
/// # Safety
///
/// This routine must only be called from handler mode while the
/// outgoing task's exception frame is on `PSP` (i.e. from PendSV or
/// another exception taken from thread mode with `SPSEL = 1`).  Both
/// pointers must be valid, aligned, and point to writable storage.
#[cfg(target_arch = "arm")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __caplet_switch(old_sp: *mut usize, new_sp: *mut usize) {
    unsafe {
        core::arch::asm!(
            // The handler runs on MSP; the task frame lives on PSP.
            "mrs r2, psp",
            "stmdb r2!, {{r4-r11}}",
            "str r2, [{0}]",
            "ldr r2, [{1}]",
            "msr psp, r2",
            "ldmia r2!, {{r4-r11}}",
            "msr psp, r2",
            // Exception return: thread mode, non-secure/secure agnostic
            // basic frame, process stack pointer.
            "ldr lr, =0xFFFFFFFD",
            "bx lr",
            in(reg) old_sp,
            in(reg) new_sp,
            options(noreturn)
        );
    }
}

/// Set up a fresh task's stack so that `__caplet_switch` will start it.
///
/// The stack layout (high → low) is:
///   [xpsr, pc, lr, r12, r3, r2, r1, r0, r11, r10, r9, r8, r7, r6, r5, r4]
/// exactly matching `TaskContext` minus the explicit struct fields.
///
/// The function returns the new PSP value ready to be passed as
/// `new_sp` to `__caplet_switch`.
#[inline(never)]
pub fn build_initial_stack(stack_top: *mut u32, entry: u32, arg: u32) -> *mut u32 {
    // Stack grows down. Layout (low → high):
    //   r4 r5 r6 r7 r8 r9 r10 r11   (callee-saved)
    //   r0 r1 r2 r3 r12 lr pc xpsr  (hardware frame, exception entry)
    let mut p = stack_top;
    // Move p down past the hardware frame (8 words).
    unsafe {
        // Hardware frame (in pop order so push builds it in reverse)
        p = p.offset(-1);
        *p = 0x0100_0000; // xpsr (Thumb)
        p = p.offset(-1);
        *p = entry; // pc
        p = p.offset(-1);
        *p = 0; // lr (thread)
        p = p.offset(-1);
        *p = 0; // r12
        p = p.offset(-1);
        *p = 0; // r3
        p = p.offset(-1);
        *p = 0; // r2
        p = p.offset(-1);
        *p = 0; // r1
        p = p.offset(-1);
        *p = arg; // r0
                  // Callee-saved frame (pop order = r4..r11)
        p = p.offset(-1);
        *p = 0; // r11
        p = p.offset(-1);
        *p = 0; // r10
        p = p.offset(-1);
        *p = 0; // r9
        p = p.offset(-1);
        *p = 0; // r8
        p = p.offset(-1);
        *p = 0; // r7
        p = p.offset(-1);
        *p = 0; // r6
        p = p.offset(-1);
        *p = 0; // r5
        p = p.offset(-1);
        *p = 0; // r4
    }
    p
}

/// Run the first task. The first task's stack must already be set up
/// with `build_initial_stack` and `sp` set to the result. This routine
/// "returns" into the first task by restoring its hardware frame from
/// the stack.
#[cfg(target_arch = "arm")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __caplet_start_first(new_sp: *mut usize) -> ! {
    unsafe {
        core::arch::asm!(
            // Select PSP and make it the active stack.
            "msr psp, {0}",
            "movs r0, #2",
            "msr control, r0",
            "isb",
            // Pop callee-saved.
            "pop {{r4-r11}}",
            // Pop the hardware frame by hand and jump to PC.
            "ldr r0, [sp, #0]",
            "ldr r1, [sp, #4]",
            "ldr r2, [sp, #8]",
            "ldr r3, [sp, #12]",
            "ldr r12, [sp, #16]",
            "ldr lr, [sp, #20]",
            "ldr r4, [sp, #24]",
            "ldr r5, [sp, #28]",
            "add sp, sp, #32",
            "bx r4",
            in(reg) new_sp,
            options(noreturn)
        );
    }
}

/// Pend a PendSV exception to request a context switch.
#[cfg(target_arch = "arm")]
#[inline(always)]
pub fn pend_pendsv() {
    unsafe {
        core::arch::asm!(
            // Preserve the caller's PRIMASK instead of unconditionally
            // enabling interrupts on the way out.
            "mrs r2, primask",
            "cpsid i",
            "ldr r0, =0xE000ED04",      // ICSR
            "movs r1, #0x10000000",     // PENDSVSET
            "str r1, [r0]",
            "dsb",
            "isb",
            "msr primask, r2",
            out("r0") _,
            out("r1") _,
            out("r2") _,
            options(nostack, preserves_flags)
        );
    }
}

/// Set the process stack pointer to `sp`. Use this when starting the
/// first task or changing tasks in handler mode.
#[cfg(target_arch = "arm")]
#[inline(always)]
pub unsafe fn set_psp(sp: u32) {
    unsafe {
        core::arch::asm!("msr psp, {0}", in(reg) sp, options(nostack, preserves_flags));
    }
}

/// Read the current process stack pointer.
#[cfg(target_arch = "arm")]
#[inline(always)]
pub fn read_psp() -> u32 {
    let sp: u32;
    unsafe {
        core::arch::asm!("mrs {0}, psp", out(reg) sp, options(nostack, preserves_flags));
    }
    sp
}

/// Pend a PendSV exception to request a context switch (host stub).
#[cfg(not(target_arch = "arm"))]
#[inline(always)]
pub fn pend_pendsv() {}

/// Read the current process stack pointer (host stub).
#[cfg(not(target_arch = "arm"))]
#[inline(always)]
pub fn read_psp() -> u32 {
    0
}

/// Set the process stack pointer (host stub).
///
/// # Safety
///
/// The host stub performs no write, but the signature matches the
/// architectural operation and callers must still supply a valid PSP.
#[cfg(not(target_arch = "arm"))]
#[inline(always)]
pub unsafe fn set_psp(_sp: u32) {}

/// Trigger a context switch via PendSV (legacy entry-point).
pub fn trigger_context_switch() {
    pend_pendsv();
}

/// Holder for the current task's PSP slot. Stored in a `static` so
/// `__caplet_switch` and the PendSV handler can reach it.
pub struct PspSlot {
    inner: UnsafeCell<*mut u32>,
}

unsafe impl Sync for PspSlot {}
unsafe impl Send for PspSlot {}

impl PspSlot {
    /// Create a new slot pointing at the given location.
    pub const fn new(ptr: *mut u32) -> Self {
        Self {
            inner: UnsafeCell::new(ptr),
        }
    }

    /// Set the underlying pointer.
    pub fn set(&self, ptr: *mut u32) {
        unsafe {
            *self.inner.get() = ptr;
        }
    }

    /// Get the underlying pointer.
    pub fn get(&self) -> *mut u32 {
        unsafe { *self.inner.get() }
    }
}

/// A handler invoked from the architecture exception vector.
///
/// The routine runs in handler mode with the interrupted task's frame
/// on `PSP`.  It may call [`__caplet_switch`]; when it returns normally
/// the interrupted task is resumed.
pub type ExceptionHook = unsafe extern "C" fn();

static PENDSV_HOOK: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());
static SVC_HOOK: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Install the handler called by the PendSV vector.  The scheduler
/// installs its pick-next-task routine here during boot.
pub fn set_pendsv_handler(hook: Option<ExceptionHook>) {
    let ptr = match hook {
        Some(f) => f as *mut (),
        None => core::ptr::null_mut(),
    };
    PENDSV_HOOK.store(ptr, Ordering::Release);
}

/// Install the handler called by the SVCall vector.  The kernel's
/// syscall entry point is installed here once, after the heap and the
/// kernel object are ready.
pub fn set_svc_handler(hook: Option<ExceptionHook>) {
    let ptr = match hook {
        Some(f) => f as *mut (),
        None => core::ptr::null_mut(),
    };
    SVC_HOOK.store(ptr, Ordering::Release);
}

/// PendSV vector entry point.  A board's vector table may reference
/// this symbol directly, or Cortex-M-RT's weak `PendSV` is overridden
/// by this strong definition.
#[cfg(all(target_arch = "arm", not(test)))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn PendSV() {
    let p = PENDSV_HOOK.load(Ordering::Acquire);
    if !p.is_null() {
        let hook: ExceptionHook = unsafe { core::mem::transmute(p) };
        unsafe { hook() };
    }
}

/// SVCall vector entry point.  The hook is responsible for decoding
/// the SVC immediate and dispatching the syscall.
#[cfg(all(target_arch = "arm", not(test)))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SVCall() {
    let p = SVC_HOOK.load(Ordering::Acquire);
    if !p.is_null() {
        let hook: ExceptionHook = unsafe { core::mem::transmute(p) };
        unsafe { hook() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On a host (non-Cortex-M) we cannot execute the asm, but we can
    /// verify the stack-frame builder produces the expected layout.
    #[test]
    fn initial_stack_layout() {
        // 256-byte aligned buffer
        let mut buf = [0u32; 16];
        let top = buf.as_mut_ptr().wrapping_add(buf.len());
        let p = build_initial_stack(top, 0xDEAD_BEEF, 0x1234_5678);
        // The bottom 8 words are r4..r11.
        unsafe {
            assert_eq!(*p.offset(0), 0); // r4
            assert_eq!(*p.offset(1), 0); // r5
            assert_eq!(*p.offset(2), 0); // r6
            assert_eq!(*p.offset(3), 0); // r7
            assert_eq!(*p.offset(4), 0); // r8
            assert_eq!(*p.offset(5), 0); // r9
            assert_eq!(*p.offset(6), 0); // r10
            assert_eq!(*p.offset(7), 0); // r11
                                         // Hardware frame starts at offset 8 (low address).
            assert_eq!(*p.offset(8), 0x1234_5678); // r0
            assert_eq!(*p.offset(9), 0); // r1
            assert_eq!(*p.offset(10), 0); // r2
            assert_eq!(*p.offset(11), 0); // r3
            assert_eq!(*p.offset(12), 0); // r12
                                          // The hardware exception frame stores the *thread* LR.  The
                                          // EXC_RETURN value is supplied by `__caplet_switch` itself
                                          // (and `__caplet_start_first` jumps directly), so it must not
                                          // be placed in the restored exception frame.
            assert_eq!(*p.offset(13), 0); // thread lr
            assert_eq!(*p.offset(14), 0xDEAD_BEEF); // pc
            assert_eq!(*p.offset(15), 0x0100_0000); // xpsr
        }
    }
}
