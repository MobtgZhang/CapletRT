use caplet_macros_exports::kernel_init;

/// Minimal hello-world for RISC-V on QEMU virt.
///
/// This board provides the `#[entry]` equivalent for the RISC-V port:
/// a `rust_main` function that the port calls after early init.
///
/// The boot flow is:
///   reset → riscv_boot() → early_init (PMP, BSS, trap vec)
///         → rust_main() → kernel_init() → scheduler → idle

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    // Initialize the kernel and run the scheduler.
    // The kernel takes over from here.
    kernel_init();

    // If the kernel ever returns (it shouldn't), halt.
    loop {
        #[cfg(target_arch = "riscv")]
        unsafe {
            core::arch::asm!("wfi", options(nostack, preserves_flags));
        }
        #[cfg(not(target_arch = "riscv"))]
        {
            core::hint::spin_loop();
        }
    }
}

/// Provide a panic handler for the host test build.
#[cfg(not(any(target_arch = "riscv", target_os = "none")))]
fn main() {
    println!("hello-riscv: host test build");
}
