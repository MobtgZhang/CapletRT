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
///
/// Gated on BOTH the `panic-handler` Cargo feature and `not(test)`, for
/// two different reasons:
///
/// * The feature lets a std-linked dependent (e.g. a host test that pulls
///   this crate in transitively) opt out. Such a build is not `cfg(test)`
///   itself, so `not(test)` alone would not help.
/// * `not(test)` covers this crate's own unit tests, which link `std` and
///   still see the default feature set.
///
/// In either case `std` already defines `panic_impl` and a second
/// definition is a hard error (`E0152`).
#[cfg(all(feature = "panic-handler", target_os = "none", not(test)))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Helper invoked explicitly by code that wants to halt after a fatal
/// error.
#[inline(never)]
pub fn cortex_m_panic() -> ! {
    loop {
        core::hint::spin_loop();
    }
}
