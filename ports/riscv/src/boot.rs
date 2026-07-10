//! Boot and initialization for RISC-V S-mode.
//!
//! On QEMU virt the boot flow is:
//!   1. Reset → jumps to 0x1000 (or entry symbol in ELF)
//!   2. `riscv_boot()` runs (called from the entry point)
//!   3. `Arch::early_init()` — PMP, interrupts off, mtvec
//!   4. (Optional) `Arch::enable_mmu()` — Sv39 page table
//!   5. Jump to `rust_main()` → kernel init
//!
//! The boot functions are only compiled for the RISC-V target.
//! Host builds (cargo test) use kernel_bridge.rs::rust_main() instead.

/// Physical base of main RAM on QEMU virt.
pub const DRAM_BASE: u32 = 0x8000_0000;
/// Size of main RAM on QEMU virt (128 MiB).
pub const DRAM_SIZE: u32 = 0x0800_0000;
/// CLINT MMIO base on QEMU virt.
pub const CLINT_BASE: u32 = 0x0200_0000;
/// PLIC MMIO base on QEMU virt.
pub const PLIC_BASE: u32 = 0x0C00_0000;
/// UART0 MMIO base on QEMU virt.
pub const UART0_BASE: u32 = 0x1000_0000;

/// Main entry point after reset. Called from the board crate's entry
/// vector (boot code in `.S` or `crt0`).
///
/// This function is only compiled for the RISC-V target.
/// On host builds, use `kernel_bridge::rust_main()` instead.
#[cfg(target_arch = "riscv")]
pub fn riscv_boot() {
    // 1. Run the architecture early init (PMP, interrupts off, trap vec).
    crate::arch::RiscVArch::early_init();

    // 2. Zero BSS. The linker script provides _sbss / _ebss symbols.
    zero_bss();

    // 3. (Optional) Set up the MMU if enabled by the board config.
    //    For QEMU virt, we run in flat physical-address mode.
    //    Boards that need virtual addressing call enable_mmu explicitly.

    // 4. Hand control to the kernel bridge.
    crate::kernel_bridge::rust_main();
}

/// Zero the BSS section marked by linker symbols.
#[cfg(target_arch = "riscv")]
fn zero_bss() {
    extern "C" {
        static mut _sbss: u8;
        static mut _ebss: u8;
    }
    unsafe {
        let start = core::ptr::addr_of_mut!(_sbss);
        let end = core::ptr::addr_of_mut!(_ebss);
        if end > start {
            core::ptr::write_bytes(start, 0, end.offset_from(start) as usize);
        }
    }
}

/// Boot the kernel with an explicit MMU configuration.
///
/// This is the variant used by boards that require virtual memory.
#[cfg(target_arch = "riscv")]
pub fn riscv_boot_with_mmu(cfg: &caplet_kernel::arch::MmuConfig) {
    crate::arch::RiscVArch::early_init();
    zero_bss();
    crate::arch::RiscVArch::enable_mmu(cfg);
    crate::kernel_bridge::rust_main();
    let _ = cfg;
}

/// Host stub: returns a dummy value so the crate compiles on non-RISC-V.
#[cfg(not(target_arch = "riscv"))]
pub fn riscv_boot() {
    // No-op on host.
}

/// Host stub for riscv_boot_with_mmu.
#[cfg(not(target_arch = "riscv"))]
pub fn riscv_boot_with_mmu(_cfg: &caplet_kernel::arch::MmuConfig) {}
