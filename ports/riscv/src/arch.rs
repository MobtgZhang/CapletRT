//! `Arch` trait implementation for RISC-V.
//!
//! Implements [`caplet_kernel::arch::Arch`] for the RISC-V target,
//! targeting S-mode (supervisor) operation. The implementation follows
//! the same pattern as [`caplet_cortex_a`] and [`caplet_cortex_m`].
//!
//! QEMU virt board address map:
//!   0x0200_0000  CLINT (core-local interruptor)
//!   0x0c00_0000  PLIC (platform-level interrupt controller)
//!   0x1000_0000  UART0 (NS16550a)
//!   0x8000_0000  DRAM start
//!   0x8000_0000  Kernel image (linked here by link.x)

extern crate alloc;

use caplet_kernel::arch::{Arch, MmuConfig};
use crate::clint::Clint;
use crate::plic::Plic;

/// RISC-V interrupt IDs matching the PLIC on QEMU virt.
pub const PLIC_IRQ_UART0: u32 = 10;

/// RISC-V hart ID. On QEMU virt single-core, this is always 0.
pub const HART_ID: u32 = 0;

/// Architecture implementation for RISC-V (S-mode).
///
/// QEMU virt starts us in S-mode with MTVEC already set to the reset
/// vector. We configure PMP, interrupts, and (optionally) MMU.
pub struct RiscVArch;

impl Arch for RiscVArch {
    fn name() -> &'static str {
        #[cfg(feature = "rv64gc")]
        return "riscv64gc";
        #[cfg(feature = "rv32imac")]
        return "riscv32imac";
        #[cfg(all(not(feature = "rv64gc"), not(feature = "rv32imac")))]
        return "riscv32imac";
    }

    fn max_cpus() -> u32 {
        1 // QEMU virt single-core; SMP supported via CLINT msip.
    }

    fn early_init() {
        // 1. Disable all interrupts (clear MIE/SIE).
        disable_interrupts();

        // 2. Configure PMP (Physical Memory Protection) to allow
        //    read/write/execute on the entire DRAM region.
        configure_pmp();

        // 3. Set machine timer (CLINT) to zero (already zero on QEMU,
        //    but explicitly initialize for determinism).
        Clint::init();

        // 4. Configure PLIC to mask all external interrupts initially.
        Plic::init();

        // 5. Set stvec to the global trap vector (trap entry).
        //    In a full implementation this points to a vector table.
        //    For now we use Direct mode pointing to `__riscv_trap_vector`.
        set_trap_vector();
    }

    fn enable_mmu(_cfg: &MmuConfig) {
        // MMU is not used on the initial RISC-V port (no Sv39/Sv48
        // page table setup). The kernel is linked in the physical
        // address space directly. A follow-up task would implement
        // Sv39 for RV64 and wire up satp CSR here.
        #[cfg(feature = "rv64gc")]
        {
            // Placeholder: would set satp = (VMID << 44) | (PPN << 4) | MODE_Sv39
        }
    }

    unsafe fn context_switch(prev: *mut u32, next: *const u32) {
        #[cfg(target_arch = "riscv")]
        {
            unsafe {
                core::arch::asm!(
                    // Save callee-saved registers (s0-s11 = 12 regs).
                    // In the Rust ABI these map to the TaskContext struct.
                    "sd s0,  0({sp})",
                    "sd s1,  8({sp})",
                    "sd s2,  16({sp})",
                    "sd s3,  24({sp})",
                    "sd s4,  32({sp})",
                    "sd s5,  40({sp})",
                    "sd s6,  48({sp})",
                    "sd s7,  56({sp})",
                    "sd s8,  64({sp})",
                    "sd s9,  72({sp})",
                    "sd s10, 80({sp})",
                    "sd s11, 88({sp})",
                    // Save sepc (trapped PC) so we can resume correctly.
                    "csrr t0, sepc",
                    "sd t0, 96({sp})",
                    // Save sstatus (interrupt enable, etc.).
                    "csrr t0, sstatus",
                    "sd t0, 104({sp})",
                    // Store the new sp (as saved context) to `prev` output slot.
                    "sd sp, 0({prev})",
                    // Load the new sp from the `next` context pointer.
                    "ld sp, 0({next})",
                    // Restore sstatus first (MIE bit restored with it).
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
                    // Return via sret — sepc already restored.
                    "sret",
                    sp = inout(reg) prev as usize,
                    prev = in(reg) prev as usize,
                    next = in(reg) next as usize,
                    options(noreturn)
                );
            }
        }
        #[cfg(not(target_arch = "riscv"))]
        {
            // Host test stub — avoid compiling the asm on non-RISC-V hosts.
            let _ = prev;
            let _ = next;
            loop {
                core::hint::spin_loop();
            }
        }
    }

    unsafe fn start_first_task(next: *const u32) -> ! {
        #[cfg(target_arch = "riscv")]
        {
            unsafe {
                // Load the initial sp from the context pointed to by `next`.
                // The context was built by `build_initial_stack` with:
                //   sp → top of stack (highest address)
                //   sepc → entry point
                //   sstatus → SIE bit cleared
                // We set sp and jump to sepc via mret.
                core::arch::asm!(
                    "ld sp, 0({next})",
                    "csrr a0, sstatus",
                    "csrr a1, sepc",
                    // Restore sstatus (clears SIE on first entry).
                    "csrw sstatus, a0",
                    // sepc already holds the entry point; mret jumps there.
                    "mret",
                    next = in(reg) next as usize,
                    options(noreturn)
                );
            }
        }
        #[cfg(not(target_arch = "riscv"))]
        {
            // Host test stub.
            loop {
                core::hint::spin_loop();
            }
        }
    }

    fn ipi_send(_target: caplet_kernel::cpu::CpuId, _reason: caplet_kernel::cpu::IpiReason, _arg: u32) {
        // CLINT msip: write 1 to bit 0 of msip hart register.
        Clint::send_software_irq(HART_ID);
    }

    fn ipi_ack(_cpu: caplet_kernel::cpu::CpuId) {
        // Clear the software interrupt by writing 0 to msip.
        Clint::clear_software_irq(HART_ID);
    }

    fn fence() {
        #[cfg(target_arch = "riscv")]
        {
            core::arch::asm!("fence iorw, iorw", options(nostack));
        }
        #[cfg(not(target_arch = "riscv"))]
        {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        }
    }

    fn now_ticks() -> u64 {
        Clint::mtime() as u64
    }

    fn request_reschedule() {
        // Trigger a software interrupt via CLINT.
        Clint::send_software_irq(HART_ID);
    }

    fn wait_for_interrupt() {
        #[cfg(target_arch = "riscv")]
        {
            core::arch::asm!("wfi", options(nostack, preserves_flags));
        }
        #[cfg(not(target_arch = "riscv"))]
        {
            // Host: just yield.
            core::hint::spin_loop();
        }
    }
}

/// Disable all interrupts (SIE bit in sstatus).
#[cfg(target_arch = "riscv")]
fn disable_interrupts() {
    unsafe {
        core::arch::asm!(
            "csrrc t0, sstatus, {mie}",
            "csrrc t0, sie, {mie}",
            mie = const 0x2, // SIE mask
            options(nostack, preserves_flags)
        );
    }
}

/// Configure PMP (Physical Memory Protection) to allow full access
/// to the entire DRAM region (0x8000_0000..0x9000_0000 on QEMU virt).
/// This is required before accessing memory in S-mode.
#[cfg(target_arch = "riscv")]
fn configure_pmp() {
    unsafe {
        // pmpaddr0 = top address of region (ADDR_MAX, encoded as A<<2+R/W/X)
        // For QEMU virt: DRAM starts at 0x8000_0000, size = 128 MiB.
        // Encoded as: (0x8000_0000 + 0x0800_0000 - 1) = 0x87FF_FFFF
        core::arch::asm!(
            "li t0, 0x{addr}",
            "csrw pmpaddr0, t0",
            // pmpcfg0: Entry 0 = TOR, RWX = 0x07 (read + write + execute)
            "li t0, 0x{cfg}",
            "csrw pmpcfg0, t0",
            addr = const 0x87FF_FFFFu32,
            cfg = const 0x0F, // PMP_A_TOR | PMP_R | PMP_W | PMP_X
            options(nostack, preserves_flags)
        );
    }
}

/// Set the trap vector base address (stvec).
/// Points to `__riscv_trap_vector` defined in context.rs.
#[cfg(target_arch = "riscv")]
fn set_trap_vector() {
    extern "C" {
        fn __riscv_trap_vector();
    }
    unsafe {
        let addr = __riscv_trap_vector as usize;
        core::arch::asm!(
            "csrw stvec, {0}",
            in(reg) addr,
            options(nostack, preserves_flags)
        );
    }
}

// Host stubs for non-RISC-V builds (cargo test on host).
#[cfg(not(target_arch = "riscv"))]
fn disable_interrupts() {}
#[cfg(not(target_arch = "riscv"))]
fn configure_pmp() {}
#[cfg(not(target_arch = "riscv"))]
fn set_trap_vector() {}
