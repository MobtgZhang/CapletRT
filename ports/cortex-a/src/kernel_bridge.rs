//! Bridge between the Cortex-A port and the kernel core.
//!
//! docv1.md §14: the port performs the actual context switch; the
//! kernel decides *what* to run next. This file exposes the entry
//! point that the boot code jumps to and forwards to the kernel.

extern crate alloc;

use crate::mmu::Mmu;

/// Entry point. Called from `rust_main_wrapper` after MMU/cache
/// setup. Currently a passthrough to `caplet_kernel::cap_init` —
/// the real kernel will hand execution to the scheduler.
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    use caplet_kernel::arch::Arch;
    unsafe {
        // Enable GIC.
        crate::gic::enable();
        // Cortex-A Arch impl lives in mmu.rs.
        let _ = Mmu::early_init;
        Mmu::early_init();
    }
    // Activate the MMU instance the board set up.
    install_mmu();
    caplet_kernel::cap_init();
    // Schedule forever.
    loop {
        #[cfg(target_arch = "arm")]
        unsafe {
            core::arch::asm!("wfi", options(nostack, preserves_flags));
        }
        #[cfg(not(target_arch = "arm"))]
        core::hint::spin_loop();
    }
}

/// Install the Cortex-A MMU implementation as the kernel's MMU.
pub fn install_mmu() -> &'static mut Mmu {
    static mut MMU: Mmu = Mmu::new();
    unsafe { &mut MMU }
}
