//! `Arch` trait implementation for AArch64.

use caplet_kernel::arch::{Arch, MmuConfig};
use caplet_kernel::cpu::CURRENT_IPI_ARG;
use caplet_kernel::cpu::{CpuId, IpiReason};
use core::sync::atomic::Ordering;

/// AArch64 port for QEMU virt / Renode Cortex-A53.
pub struct AArch64Arch;

impl Arch for AArch64Arch {
    fn name() -> &'static str {
        "aarch64"
    }

    fn max_cpus() -> u32 {
        1
    }

    fn early_init() {
        #[cfg(target_arch = "aarch64")]
        unsafe {
            core::arch::asm!("msr daifset, #0xf", options(nomem, nostack));
            core::arch::asm!("isb", options(nomem, nostack));
        }
        // Program the distributor/redistributor and leave the timer
        // stopped.  The cooperative scheduler does not depend on timer
        // interrupts, but a future exception-vector path can enable the
        // PPI without repeating controller discovery.
        let _ = crate::gic::init();
        crate::tick::stop();
    }

    fn enable_mmu(_cfg: &MmuConfig) {
        #[cfg(target_arch = "aarch64")]
        // SAFETY: boot is single-threaded and the identity map covers
        // the kernel image, heap and all MMIO used by the port.
        unsafe {
            crate::mmu::enable_native_paging();
        }
    }

    unsafe fn context_switch(prev: *mut usize, next: *const usize) {
        #[cfg(target_arch = "aarch64")]
        {
            // SAFETY: forwarded from the trait contract.
            unsafe { crate::context::__caplet_switch(prev, next) }
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            let _ = (prev, next);
            core::hint::spin_loop();
        }
    }

    unsafe fn start_first_task(next: *const usize) -> ! {
        #[cfg(target_arch = "aarch64")]
        {
            // SAFETY: forwarded from the trait contract.
            unsafe { crate::context::__caplet_start_first(next) }
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            let _ = next;
            loop {
                core::hint::spin_loop();
            }
        }
    }

    fn ipi_send(target: CpuId, reason: IpiReason, arg: u32) {
        CURRENT_IPI_ARG.store(arg, Ordering::Release);
        if target.0 < 8 {
            crate::gic::send_sgi(reason as u32, 1u8 << target.0);
        }
    }

    fn ipi_ack(_cpu: CpuId) {
        crate::gic::eoi(crate::gic::IPI_RESCHEDULE_IRQ);
    }

    fn fence() {
        #[cfg(target_arch = "aarch64")]
        unsafe {
            core::arch::asm!("dmb sy", options(nostack));
        }
        #[cfg(not(target_arch = "aarch64"))]
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    fn now_ticks() -> u64 {
        crate::tick::now_ticks()
    }

    fn request_reschedule() {
        // SGI 0 targets the current CPU.  Interrupts remain masked in
        // the cooperative bring-up path, so this is a wake-up hint for
        // WFE and the preemptive path rather than a mandatory switch.
        crate::gic::send_sgi(crate::gic::IPI_RESCHEDULE_IRQ, 1);
        #[cfg(target_arch = "aarch64")]
        // SAFETY: SEV is an execution hint with no memory side effects.
        unsafe {
            core::arch::asm!("sev", options(nomem, nostack));
        }
    }

    fn wait_for_interrupt() {
        #[cfg(target_arch = "aarch64")]
        unsafe {
            core::arch::asm!("wfe", options(nomem, nostack));
        }
        #[cfg(not(target_arch = "aarch64"))]
        core::hint::spin_loop();
    }
}
