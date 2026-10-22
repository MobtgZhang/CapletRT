//! `Arch` trait implementation for RISC-V.
//!
//! The port runs in supervisor mode (S-mode). QEMU `virt` and the
//! Renode `riscv_virt` platform both start the hart in machine mode, so
//! `boot.rs` performs the M-mode setup (PMP, delegation, `mret`) before
//! any code in this module runs.
//!
//! QEMU virt address map:
//!   0x0200_0000  CLINT (core-local interruptor)
//!   0x0c00_0000  PLIC (platform-level interrupt controller)
//!   0x1000_0000  UART0 (NS16550a)
//!   0x8000_0000  DRAM start (kernel image is linked here)

extern crate alloc;

use caplet_kernel::arch::{Arch, MmuConfig};
use caplet_kernel::cpu::{CpuId, IpiReason};

use crate::clint::Clint;
use crate::plic::Plic;

/// RISC-V interrupt IDs matching the PLIC on QEMU virt.
pub const PLIC_IRQ_UART0: u32 = 10;

/// RISC-V hart ID. On QEMU virt single-core, this is always 0.
pub const HART_ID: u32 = 0;

/// Architecture implementation for RISC-V (S-mode).
pub struct RiscVArch;

impl Arch for RiscVArch {
    fn name() -> &'static str {
        #[cfg(target_arch = "riscv64")]
        {
            "riscv64gc"
        }
        #[cfg(target_arch = "riscv32")]
        {
            "riscv32imac"
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            "riscv-host-stub"
        }
    }

    fn max_cpus() -> u32 {
        1
    }

    fn early_init() {
        disable_interrupts();
        // PMP was configured by the M-mode reset stub. Re-programming
        // it here would require M-mode and would fault from S-mode.
        Plic::init();
        set_trap_vector();
    }

    fn enable_mmu(cfg: &MmuConfig) {
        // Install the native identity map when the caller supplied a
        // real kernel base.  `MmuConfig::no_mmu()` keeps the early PMP
        // bring-up path (used by diagnostics) untouched.
        if cfg.kernel_vaddr != 0 {
            crate::mmu::enable_native_paging();
        }
    }

    unsafe fn context_switch(prev: *mut usize, next: *const usize) {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            // SAFETY: forwarded contract from the trait.
            unsafe { crate::context::__caplet_switch(prev, next) }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = (prev, next);
            core::hint::spin_loop();
        }
    }

    unsafe fn start_first_task(next: *const usize) -> ! {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            // SAFETY: forwarded contract from the trait.
            unsafe { crate::context::__caplet_start_first(next) }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = next;
            loop {
                core::hint::spin_loop();
            }
        }
    }

    fn ipi_send(_target: CpuId, _reason: IpiReason, _arg: u32) {
        Clint::send_software_irq(HART_ID);
    }

    fn ipi_ack(_cpu: CpuId) {
        Clint::clear_software_irq(HART_ID);
    }

    fn fence() {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            // SAFETY: a full fence has no memory-safety preconditions.
            unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) };
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        }
    }

    fn now_ticks() -> u64 {
        Clint::mtime()
    }

    fn request_reschedule() {
        crate::context::pend_software_irq();
    }

    fn wait_for_interrupt() {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            // SAFETY: WFI may be executed at any privilege level.
            unsafe { core::arch::asm!("wfi", options(nostack, preserves_flags)) };
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            core::hint::spin_loop();
        }
    }
}

/// Disable supervisor interrupts (`sstatus.SIE`).
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
fn disable_interrupts() {
    // SAFETY: clearing SIE is always valid in S-mode.
    unsafe {
        core::arch::asm!(
            "csrrc zero, sstatus, {sie}",
            sie = const 0x2usize,
            options(nostack, preserves_flags)
        );
    }
}

/// Set the trap vector base address (`stvec`) to the supervisor trap
/// entry point.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
fn set_trap_vector() {
    let addr = crate::context::__riscv_trap_vector as *const () as usize;
    // SAFETY: `__riscv_trap_vector` is an aligned assembly entry point.
    unsafe {
        core::arch::asm!(
            "csrw stvec, {0}",
            in(reg) addr,
            options(nostack, preserves_flags)
        );
    }
}

// Host stubs for non-RISC-V builds (cargo test on the development host).
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
fn disable_interrupts() {}
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
fn set_trap_vector() {}
