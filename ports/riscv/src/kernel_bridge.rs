//! Bridge between the RISC-V port and the kernel core.
//!
//! This file exposes the entry point that `riscv_boot()` jumps to and
//! forwards to `caplet_kernel::cap_init()`.

extern crate alloc;

use crate::arch::RiscVArch;

/// Entry point called from `riscv_boot()` after early init.
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    use caplet_kernel::arch::Arch;

    // Enable the UART so we can print early boot messages.
    crate::uart::Uart::init();
    crate::uart::Uart::put_line("CapletRT booting on RISC-V");

    // Initialize the architecture.
    RiscVArch::early_init();
    RiscVArch::enable_mmu(&caplet_kernel::arch::MmuConfig::no_mmu());

    // Run the kernel initialization.
    caplet_kernel::cap_init();

    // If we ever return from cap_init (which we shouldn't in a normal
    // boot), park the CPU.
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
