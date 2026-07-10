//! Kernel initialization

use crate::kernel::Kernel;
use crate::kernel_config::KernelConfig;
use core::mem::MaybeUninit;

/// Initialize the CapletRT kernel
///
/// This function sets up the kernel and must be called before
/// any other kernel functions.
///
/// # Safety
///
/// This function must be called only once, early in boot.
pub fn cap_init() -> &'static mut Kernel {
    // Create default configuration for QEMU
    let config = KernelConfig::qemu();

    // Initialize global kernel using MaybeUninit for safety
    unsafe {
        static mut KERNEL_INSTANCE: MaybeUninit<Kernel> = MaybeUninit::uninit();
        KERNEL_INSTANCE.write(Kernel::new(config));
        let kernel = KERNEL_INSTANCE.assume_init_mut();
        kernel.init();
        kernel
    }
}

/// Initialize the kernel with custom config
pub fn cap_init_with_config(config: KernelConfig) -> &'static mut Kernel {
    unsafe {
        static mut KERNEL_INSTANCE: MaybeUninit<Kernel> = MaybeUninit::uninit();
        KERNEL_INSTANCE.write(Kernel::new(config));
        let kernel = KERNEL_INSTANCE.assume_init_mut();
        kernel.init();
        kernel
    }
}

/// Kernel default entry point stub. The `#[caplet::main]` attribute
/// macro generates its own `caplet_kernel_entry` for crates that want
/// a custom boot path. This symbol is intentionally guarded by
/// `#[link_section]` so the board crate's macro expansion wins during
/// the link.
#[doc(hidden)]
#[no_mangle]
#[link_section = ".text.caplet_kernel_default"]
pub extern "C" fn _caplet_kernel_default_entry() -> ! {
    let kernel = cap_init();
    kernel.start()
}