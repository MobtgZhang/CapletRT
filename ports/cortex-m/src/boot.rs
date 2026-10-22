//! Boot and initialization for Cortex-M

/// Memory layout for QEMU Cortex-M3/M4
/// Flash start address
pub const FLASH_START: u32 = 0x0000_0000;
/// RAM start address
pub const RAM_START: u32 = 0x2000_0000;
/// RAM size in bytes
pub const RAM_SIZE: u32 = 64 * 1024;

/// Initialize the Cortex-M processor
pub fn cortex_m_boot() {
    // Disable all interrupts
    disable_interrupts();

    // Configure vector table offset
    // In real hardware, this would set SCB->VTOR

    // Enable FPU if present (Cortex-M4)
    #[cfg(feature = "cortex_m4")]
    {
        enable_fpu();
    }

    // Initialize memory
    init_memory();

    // The board crate (or the bin's `#[entry]`) is expected to define
    // `rust_main`. If we got here without one, fall back to spinning.
    extern "Rust" {
        fn rust_main() -> !;
    }
    unsafe { rust_main() }
}

/// Disable all interrupts
fn disable_interrupts() {
    #[cfg(target_arch = "arm")]
    unsafe {
        core::arch::asm!("cpsid i", "dsb", "isb", options(nostack, preserves_flags));
    }
}

/// Enable the FPU (Cortex-M4 with FPU)
#[cfg(all(feature = "cortex_m4", target_arch = "arm"))]
fn enable_fpu() {
    // CPACR: set bits 20-23 (CP10 and CP11) for full access
    unsafe {
        core::arch::asm!(
            "ldr r0, =0xE000ED88",
            "ldr r1, [r0]",
            "orr r1, r1, #0x00F00000",
            "str r1, [r0]",
            "dsb",
            "isb",
            options(nostack, preserves_flags)
        );
    }
}

/// Enable the FPU (host stub).
#[cfg(all(feature = "cortex_m4", not(target_arch = "arm")))]
fn enable_fpu() {}

/// Initialize memory
fn init_memory() {
    // Cortex-M-RT already clears BSS before `main`, but boards that
    // provide their own reset vector call this function explicitly.
    // Clearing an already-zero range is harmless and makes the port
    // correct for both boot paths.
    #[cfg(all(target_arch = "arm", not(test)))]
    unsafe {
        unsafe extern "C" {
            static mut _sbss: u8;
            static mut _ebss: u8;
        }
        let start = core::ptr::addr_of_mut!(_sbss) as *mut u32;
        let end = core::ptr::addr_of_mut!(_ebss) as *mut u32;
        let mut p = start;
        while p < end {
            core::ptr::write_volatile(p, 0);
            p = p.add(1);
        }
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    }
}

/// Initialize the kernel
pub fn kernel_init() {
    let _kernel = caplet_kernel::cap_init();
    crate::tickless::install();
}
