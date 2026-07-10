//! Panic handler for Cortex-M
//!
//! On `no_std` targets (Cortex-M) we install a tiny
//! `#[panic_handler]` that halts the CPU. On host builds (tests
//! driving the kernel with `std`) the panic handler is suppressed
//! because `std` already provides one and a duplicate `panic_impl`
//! lang item is a hard error.

/// Cortex-M panic handler — simply stops execution so a debugger can
/// inspect the state. We avoid pulling in `cortex-m-semihosting` so the
/// kernel TCB stays small.
#[cfg_attr(all(target_os = "none", not(test)), panic_handler)]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// Helper invoked explicitly by code that wants to halt after a fatal
/// error.
#[inline(never)]
pub fn cortex_m_panic() -> ! {
    loop {}
}