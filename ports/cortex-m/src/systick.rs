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

/// SysTick control/status register.
#[cfg(target_arch = "arm")]
const SYST_CSR: *mut u32 = 0xE000_E010 as *mut u32;
/// SysTick reload value register.
#[cfg(target_arch = "arm")]
const SYST_RVR: *mut u32 = 0xE000_E014 as *mut u32;
/// SysTick current value register.
#[cfg(target_arch = "arm")]
const SYST_CVR: *mut u32 = 0xE000_E018 as *mut u32;
/// SysTick calibration register.
#[cfg(target_arch = "arm")]
const SYST_CALIB: *const u32 = 0xE000_E01C as *const u32;

/// Reload period currently programmed into SysTick, in milliseconds.
#[cfg(target_arch = "arm")]
static LAST_RELOAD_MS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1);
/// Number of SysTick exceptions taken since the last reset.
#[cfg(target_arch = "arm")]
static IRQ_COUNT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Enable the SysTick counter.
#[cfg(target_arch = "arm")]
pub fn enable() {
    unsafe {
        // CLKSOURCE | TICKINT | ENABLE
        core::ptr::write_volatile(SYST_CSR, 0x7);
    }
}

/// Disable the SysTick counter.
#[cfg(target_arch = "arm")]
pub fn disable() {
    unsafe {
        core::ptr::write_volatile(SYST_CSR, 0);
    }
}

/// Processor-clock cycles represented by the SysTick calibration
/// (`TENMS`). Returns `0` when the platform does not provide it.
#[cfg(target_arch = "arm")]
pub fn calibrated_cycles_per_ms() -> u32 {
    let calib = unsafe { core::ptr::read_volatile(SYST_CALIB) };
    // NOREF (bit 31) means no separate reference clock, which is fine;
    // TENMS (bits 23:0) is the count for ten milliseconds.
    let ten_ms = calib & 0x00FF_FFFF;
    if ten_ms == 0 {
        0
    } else {
        (ten_ms / 10).max(1)
    }
}

/// Raw SysTick calibration register (diagnostics).
#[cfg(target_arch = "arm")]
pub fn calibration_raw() -> u32 {
    unsafe { core::ptr::read_volatile(SYST_CALIB) }
}

/// Currently programmed SysTick reload value (diagnostics).
#[cfg(target_arch = "arm")]
pub fn reload_value() -> u32 {
    unsafe { core::ptr::read_volatile(SYST_RVR) }
}

/// Cycles per millisecond used by [`reload_ms`].
///
/// The MPS2 platforms used by the QEMU/Renode matrix run at 25 MHz and
/// do not always expose a useful TENMS field, so that value is the
/// portable fallback. Boards with a different core clock can call
/// [`reload_ms_with_frequency`] directly.
#[cfg(target_arch = "arm")]
pub fn cycles_per_ms() -> u32 {
    let calibrated = calibrated_cycles_per_ms();
    if calibrated == 0 {
        25_000
    } else {
        calibrated
    }
}

/// Reload the SysTick counter with `ticks`. Also clears the current
/// value and the COUNTFLAG.
#[cfg(target_arch = "arm")]
pub fn reload(ticks: u32) {
    let reload = ticks.clamp(1, 0x00FF_FFFF);
    unsafe {
        core::ptr::write_volatile(SYST_RVR, reload);
        core::ptr::write_volatile(SYST_CVR, 0);
    }
}

/// Program SysTick for a duration in milliseconds and return the
/// duration actually represented by the 24-bit reload register.
#[cfg(target_arch = "arm")]
pub fn reload_ms(ms: u32) -> u32 {
    reload_ms_with_frequency(ms, cycles_per_ms())
}

/// Program SysTick using an explicit core-clock frequency. The return
/// value is the clamped duration in milliseconds, which is what the
/// ISR must add to the kernel clock.
#[cfg(target_arch = "arm")]
pub fn reload_ms_with_frequency(ms: u32, cycles_per_ms: u32) -> u32 {
    let cpm = cycles_per_ms.max(1);
    let requested = ms.max(1);
    let max_ms = (0x00FF_FFFFu32 / cpm).max(1);
    let actual_ms = requested.min(max_ms);
    let cycles = actual_ms.saturating_mul(cpm).clamp(1, 0x00FF_FFFF);
    LAST_RELOAD_MS.store(actual_ms, core::sync::atomic::Ordering::Release);
    reload(cycles);
    actual_ms
}

/// Initialize SysTick as a periodic millisecond time base. The first
/// deadline can subsequently be changed with [`reload_ms`].
#[cfg(target_arch = "arm")]
pub fn init(period_ms: u32) {
    disable();
    let _ = reload_ms(period_ms);
    enable();
}

/// Read the SysTick interrupt counter.
#[cfg(target_arch = "arm")]
pub fn irq_count() -> u32 {
    IRQ_COUNT.load(core::sync::atomic::Ordering::Acquire)
}

/// Reset the SysTick interrupt counter (diagnostics/tests).
#[cfg(target_arch = "arm")]
pub fn reset_irq_count() {
    IRQ_COUNT.store(0, core::sync::atomic::Ordering::Release);
}

/// SysTick exception handler.
///
/// The Cortex-M vector table expects the exported symbol `SysTick`.
/// `tickless::install` takes a function pointer to this handler so the
/// linker always pulls it out of the port archive even though the
/// vector table uses a weak `PROVIDE` alias.
#[cfg(target_arch = "arm")]
#[allow(non_snake_case)]
#[no_mangle]
#[doc(hidden)]
pub extern "C" fn SysTick() {
    IRQ_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    // One-shot tickless time base: re-arm happens in the idle hook,
    // after the scheduler has processed this expiry.
    disable();
    let elapsed = LAST_RELOAD_MS
        .load(core::sync::atomic::Ordering::Acquire)
        .max(1);
    caplet_kernel::syscall_timer::increment_ticks(elapsed);
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
/// Program SysTick for a duration in milliseconds (host stub).
pub fn reload_ms(ms: u32) -> u32 {
    ms.max(1)
}
#[cfg(not(target_arch = "arm"))]
/// Program SysTick using an explicit frequency (host stub).
pub fn reload_ms_with_frequency(ms: u32, _cycles_per_ms: u32) -> u32 {
    ms.max(1)
}
#[cfg(not(target_arch = "arm"))]
/// Initialize the SysTick time base (host stub).
pub fn init(_period_ms: u32) {}
#[cfg(not(target_arch = "arm"))]
/// Read the SysTick interrupt counter (host stub).
pub fn irq_count() -> u32 {
    0
}
#[cfg(not(target_arch = "arm"))]
/// Reset the SysTick interrupt counter (host stub).
pub fn reset_irq_count() {}
#[cfg(not(target_arch = "arm"))]
/// Raw SysTick calibration register (host stub).
pub fn calibration_raw() -> u32 {
    0
}
#[cfg(not(target_arch = "arm"))]
/// Currently programmed SysTick reload value (host stub).
pub fn reload_value() -> u32 {
    0
}
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
