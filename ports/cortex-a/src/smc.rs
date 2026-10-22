//! PSCI / SMC bindings.
//!
//! Used by the SMP boot path to power-on secondary CPUs. On QEMU
//! vexpress-a9 the default firmware implements PSCI, so we just
//! need a tiny wrapper around `smc #0`.

/// PSCI function ids (ARM DEN 0022D).
pub mod psci {
    /// Power Domain Level — used for the `version` field of every
    /// PSCI call.
    pub const VERSION: u32 = 0x8400_0000;
    /// CPU on.
    pub const CPU_ON: u32 = 0x8400_0003;

    /// Issue a PSCI call via SMC #0.
    ///
    /// # Safety
    ///
    /// Must only be executed in ARM secure monitor mode with a PSCI
    /// implementation present. `entry_point` must be a valid physical
    /// address in the target CPU's executable mapping.
    #[inline(always)]
    #[cfg(target_arch = "arm")]
    pub unsafe fn cpu_on(target_cpu: u32, entry_point: u32, context_id: u32) -> i32 {
        let ret: u32;
        core::arch::asm!(
            "smc #0",
            inout("r0") CPU_ON => ret,
            in("r1") target_cpu,
            in("r2") entry_point,
            in("r3") context_id,
            options(nostack)
        );
        ret as i32
    }

    /// Host-only stub.  PSCI is unavailable outside a bare-metal ARM
    /// target; returning -1 mirrors the PSCI `NOT_SUPPORTED` error.
    ///
    /// # Safety
    ///
    /// Kept `unsafe` to match the bare-metal API. The stub performs no
    /// memory access or privileged operation.
    #[inline(always)]
    #[cfg(not(target_arch = "arm"))]
    pub unsafe fn cpu_on(_target_cpu: u32, _entry_point: u32, _context_id: u32) -> i32 {
        -1
    }
}
