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
    #[inline(always)]
    pub unsafe fn cpu_on(target_cpu: u32, entry_point: u32, context_id: u32) -> i32 {
        let ret: u32;
        core::arch::asm!(
            "mov r0, {fid}",
            "mov r1, {tgt}",
            "mov r2, {ep}",
            "mov r3, {cid}",
            "smc #0",
            "mov {ret}, r0",
            fid = in(reg) CPU_ON,
            tgt = in(reg) target_cpu,
            ep = in(reg) entry_point,
            cid = in(reg) context_id,
            ret = out(reg) ret,
            options(nostack, preserves_flags)
        );
        ret as i32
    }
}
