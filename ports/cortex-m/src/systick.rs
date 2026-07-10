//! SysTick driver for the CapletRT Cortex-M port.
//!
//! Provides the small surface needed by the kernel for tickless idle:
//!   - `reload(ticks)` to set the next deadline
//!   - `enable()` / `disable()`
//!   - `current()` to read the current countdown
//!   - `wfi()` / `wfe()`
//!
//! All access is in-line assembly to avoid pulling `cortex-m` into the
//! kernel TCB path. On non-Cortex-M targets (host tests) the
//! functions are stubs that do nothing.

/// Enable the SysTick counter.
#[cfg(target_arch = "arm")]
pub fn enable() {
    unsafe {
        core::arch::asm!(
            "ldr r0, =0xE000E010",  // SYST_CSR
            "movs r1, #0x05",       // CLKSOURCE | ENABLE
            "str r1, [r0]",
            options(nostack, preserves_flags)
        );
    }
}

/// Disable the SysTick counter.
#[cfg(target_arch = "arm")]
pub fn disable() {
    unsafe {
        core::arch::asm!(
            "ldr r0, =0xE000E010",
            "movs r1, #0",
            "str r1, [r0]",
            options(nostack, preserves_flags)
        );
    }
}

/// Reload the SysTick counter with `ticks`. Also clears the current
/// value and the COUNTFLAG.
#[cfg(target_arch = "arm")]
pub fn reload(ticks: u32) {
    unsafe {
        core::arch::asm!(
            "ldr r0, =0xE000E014",  // SYST_RVR
            "str {val}, [r0]",
            "ldr r0, =0xE000E018",  // SYST_CVR
            "str {val}, [r0]",
            val = in(reg) ticks,
            options(nostack, preserves_flags)
        );
    }
}

/// Read the current value (countdown).
#[cfg(target_arch = "arm")]
pub fn current() -> u32 {
    let mut v: u32 = 0;
    let ptr: u32 = 0xE000E018;
    unsafe {
        core::arch::asm!(
            "ldr {val}, [{ptr}]",
            ptr = in(reg) ptr,
            val = out(reg) v,
            options(nostack, preserves_flags)
        );
    }
    v
}

/// WFI — wait for the next interrupt. Used by the idle task.
#[cfg(target_arch = "arm")]
pub fn wfi() {
    unsafe {
        core::arch::asm!("wfi", options(nostack, preserves_flags));
    }
}

/// WFE — wait for event. Lower-power than WFI on some cores.
#[cfg(target_arch = "arm")]
pub fn wfe() {
    unsafe {
        core::arch::asm!("wfe", options(nostack, preserves_flags));
    }
}

// Host stubs so the test build still links. The kernel never reaches
// these on the host (the `target_arch = "arm"` build is the real one),
// but the symbols must be present so the kernel crate compiles for
// `cargo test`.
#[cfg(not(target_arch = "arm"))]
/// Enable the SysTick counter (host stub).
pub fn enable() {}
#[cfg(not(target_arch = "arm"))]
/// Disable the SysTick counter (host stub).
pub fn disable() {}
#[cfg(not(target_arch = "arm"))]
/// Reload the SysTick counter (host stub).
pub fn reload(_ticks: u32) {}
#[cfg(not(target_arch = "arm"))]
/// Read the SysTick current value (host stub).
pub fn current() -> u32 {
    0
}
#[cfg(not(target_arch = "arm"))]
/// Wait-for-interrupt (host stub).
pub fn wfi() {}
#[cfg(not(target_arch = "arm"))]
/// Wait-for-event (host stub).
pub fn wfe() {}