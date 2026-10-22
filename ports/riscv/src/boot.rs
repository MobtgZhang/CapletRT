//! Boot and initialization for RISC-V on the QEMU `virt` machine.
//!
//! QEMU's `virt` machine starts the hart in **M-mode** at its internal
//! reset ROM and then jumps to the ELF entry point.  A supervisor-mode
//! kernel therefore cannot simply start executing: the port must
//! configure the physical memory protection unit, delegate the
//! supervisor interrupts, and perform an explicit `mret` into S-mode.
//!
//! Boot flow implemented here:
//!
//! 1. `_start` runs in M-mode, installs a machine trap vector, sets up
//!    PMP entries for DRAM/UART/CLINT/PLIC, delegates SSI/STI/SEI and
//!    S-mode `ecall`, then `mret`s into `s_mode_entry`.
//! 2. `s_mode_entry` installs the supervisor trap vector, enables the
//!    delegated interrupt sources, and calls [`riscv_boot`].
//! 3. [`riscv_boot`] clears BSS, initialises the UART, runs the
//!    architecture early init and finally calls the board's
//!    `rust_main`.
//!
//! The reset/trap stubs are written with `global_asm!` because they
//! must run before any Rust stack frame or runtime support exists.

/// Physical base of main RAM on QEMU virt.
pub const DRAM_BASE: usize = 0x8000_0000;
/// Size of main RAM on QEMU virt (128 MiB by default).
pub const DRAM_SIZE: usize = 0x0800_0000;
/// CLINT MMIO base on QEMU virt.
pub const CLINT_BASE: usize = 0x0200_0000;
/// PLIC MMIO base on QEMU virt.
pub const PLIC_BASE: usize = 0x0C00_0000;
/// UART0 MMIO base on QEMU virt.
pub const UART0_BASE: usize = 0x1000_0000;
/// CLINT timebase frequency used by QEMU virt and the Renode platform.
pub const CLINT_HZ: u64 = 10_000_000;

/// RISC-V machine-mode entry point and M→S transition.
///
/// The section name `.text.entry` is placed first by the board linker
/// script, which makes this the ELF entry point as well.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
core::arch::global_asm!(
    r#"
    .section .text.entry, "ax"
    .balign 4
    .global _start
    .type _start, @function
_start:
    /* A stack is required before the first call. */
    la      sp, _estack

    /* Machine trap vector: only used if something goes wrong before
     * the supervisor trap vector is installed. */
    la      t0, __machine_trap_vector
    csrw    mtvec, t0

    /* PMP entry 0: 128 MiB DRAM at 0x8000_0000, NAPOT, RWX. */
    li      t0, 0x20FFFFFF
    csrw    pmpaddr0, t0
    /* PMP entry 1: UART0 4 KiB, NAPOT, RW. */
    li      t0, 0x040001FF
    csrw    pmpaddr1, t0
    /* PMP entry 2: CLINT 64 KiB, NAPOT, RW. */
    li      t0, 0x00801FFF
    csrw    pmpaddr2, t0
    /* PMP entry 3: PLIC 64 MiB, NAPOT, RW. */
    li      t0, 0x037FFFFF
    csrw    pmpaddr3, t0
    /* cfg0: pmp3=NAPOT|RW, pmp2=NAPOT|RW,
     *       pmp1=NAPOT|RW, pmp0=NAPOT|RWX */
    li      t0, 0x1B1B1B1F
    csrw    pmpcfg0, t0

    /* Delegate supervisor software/timer/external interrupts (SSI,
     * STI, SEI) and supervisor ecall to S-mode. */
    li      t0, 0x222
    csrw    mideleg, t0
    li      t0, (1 << 9)
    csrw    medeleg, t0

    /* mstatus: MPP=S (bits 12:11 = 01), FS=Dirty (bits 14:13 = 11)
     * so S-mode may use the F/D extensions, MPIE=1, MIE=0.  Boards
     * built for rv32imac have no FPU and simply ignore FS. */
    li      t0, (1 << 11) | (3 << 13) | (1 << 7)
    csrw    mstatus, t0
    la      t0, s_mode_entry
    csrw    mepc, t0
    mret
    .size _start, . - _start

    /* Supervisor-mode entry.  The hart is now executing with S-mode
     * privilege, so the machine CSRs used above are no longer
     * accessible. */
    .section .text.entry, "ax"
    .balign 4
    .type s_mode_entry, @function
s_mode_entry:
    la      sp, _estack
    la      t0, __riscv_trap_vector
    csrw    stvec, t0
    /* SSIE | STIE | SEIE */
    li      t0, 0x222
    csrw    sie, t0
    /* sstatus.SIE */
    csrs    sstatus, 2
    call    riscv_boot
1:
    j       1b
    .size s_mode_entry, . - s_mode_entry

    .section .text, "ax"
    .balign 4
    .global __machine_trap_vector
    .type __machine_trap_vector, @function
__machine_trap_vector:
    csrr    a0, mcause
    csrr    a1, mepc
    csrr    a2, mtval
    call    machine_trap_handler
1:
    j       1b
    .size __machine_trap_vector, . - __machine_trap_vector
    "#
);

/// Entry point called from [`s_mode_entry`] after the M→S transition.
///
/// Clears BSS, initialises the UART, runs the architecture early init
/// and dispatches to the board's `rust_main` symbol.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[no_mangle]
pub fn riscv_boot() -> ! {
    zero_bss();
    crate::uart::Uart::init();
    crate::uart::Uart::put_line("CapletRT: RISC-V supervisor-mode entry");

    <crate::arch::RiscVArch as caplet_kernel::arch::Arch>::early_init();

    unsafe extern "C" {
        fn rust_main() -> !;
    }
    // SAFETY: the board crate provides the `rust_main` entry point and
    // never returns.
    unsafe { rust_main() }
}

/// Zero the `.bss` section marked by the linker script.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub fn zero_bss() {
    unsafe extern "C" {
        static mut _sbss: u8;
        static mut _ebss: u8;
    }
    // SAFETY: `_sbss.._ebss` is the linker-provided BSS range and is
    // writable by the kernel.
    unsafe {
        let start = core::ptr::addr_of_mut!(_sbss);
        let end = core::ptr::addr_of_mut!(_ebss);
        if end > start {
            core::ptr::write_bytes(start, 0, end.offset_from(start) as usize);
        }
    }
}

/// Fatal trap taken while still in M-mode.
///
/// This should only fire for a bug in the boot stub or for a machine
/// interrupt that the kernel does not handle.  The function prints the
/// trap cause and PC and parks the hart so QEMU/Renode remain alive
/// for a debugger.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[no_mangle]
pub extern "C" fn machine_trap_handler(cause: usize, epc: usize, tval: usize) -> ! {
    use crate::uart::Uart;
    Uart::init();
    Uart::put_line("PANIC: unhandled RISC-V machine-mode trap");
    Uart::put_str("mcause=");
    Uart::put_hex(cause as u64);
    Uart::put_str(" mepc=");
    Uart::put_hex(epc as u64);
    Uart::put_str(" mtval=");
    Uart::put_hex(tval as u64);
    Uart::put_line("");
    loop {
        // SAFETY: WFI is safe in this parking loop.
        unsafe { core::arch::asm!("wfi", options(nostack, preserves_flags)) };
    }
}

/// Host stub so the port crate still builds for `cargo test` on the
/// development machine.
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub fn riscv_boot() {}

/// Host stub for [`zero_bss`].
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub fn zero_bss() {}

/// Boot the kernel with an explicit MMU configuration.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub fn riscv_boot_with_mmu(cfg: &caplet_kernel::arch::MmuConfig) -> ! {
    <crate::arch::RiscVArch as caplet_kernel::arch::Arch>::enable_mmu(cfg);
    riscv_boot()
}

/// Host stub for [`riscv_boot_with_mmu`].
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub fn riscv_boot_with_mmu(_cfg: &caplet_kernel::arch::MmuConfig) {}
