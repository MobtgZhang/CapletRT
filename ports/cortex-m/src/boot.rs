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
    // Would use inline assembly or cortex-m crate
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
    // Zero the BSS section
    // In a real startup, this would be done in assembly
}

/// Initialize the kernel
pub fn kernel_init() {
    // In actual implementation, this would call caplet_kernel::cap_init()
}
