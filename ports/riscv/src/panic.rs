//! Panic handler for RISC-V.
//!
//! Halts the CPU on panic. On QEMU this leaves the simulator running
//! so the developer can inspect state via GDB.

/// Panic handler — halts the CPU by entering an infinite loop.
#[cfg(all(target_os = "none", not(test)))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // On RISC-V, `ebreak` with a debug trigger can be caught by QEMU.
    // Falling back to an infinite loop keeps the CPU alive for GDB inspection.
    loop {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            // Park the CPU.
            unsafe {
                core::arch::asm!("wfi", options(nostack));
            }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            core::hint::spin_loop();
        }
    }
}

/// Explicit halt — call this from fatal error paths.
#[inline(never)]
pub fn riscv_halt() -> ! {
    loop {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            unsafe {
                core::arch::asm!("wfi", options(nostack));
            }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            core::hint::spin_loop();
        }
    }
}
