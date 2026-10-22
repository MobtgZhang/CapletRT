//! Kernel initialization
//!
//! All initialisation paths publish the *same* kernel instance through
//! [`crate::kernel::init_kernel`]; there is no second `static mut`
//! instance.

use crate::kernel::{get_kernel, init_kernel, Kernel};
use crate::kernel_config::KernelConfig;

/// Initialize the CapletRT kernel
///
/// This function sets up the kernel and must be called before
/// any other kernel functions.
///
/// # Safety
///
/// This function must be called only once, early in boot.
pub fn cap_init() -> &'static mut Kernel {
    cap_init_with_config(KernelConfig::qemu())
}

/// Initialize the kernel with custom config
pub fn cap_init_with_config(config: KernelConfig) -> &'static mut Kernel {
    // Safety: boot code is single-threaded and `init_kernel` is
    // idempotent, so repeated calls return the first published
    // instance.
    unsafe { init_kernel(config) };
    unsafe { get_kernel() }.expect("kernel initialisation failed")
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
