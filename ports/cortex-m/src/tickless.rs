//! Tickless idle bridge for the Cortex-M port.
//!
//! Registers the real WFI routine as the kernel's idle hook. The
//! kernel calls this from `init_kernel` via the `set_idle_hook`
//! function pointer API; the port only needs to provide the actual
//! `unsafe extern "C" fn(ticks: u32)` implementation.

use core::sync::atomic::{AtomicBool, Ordering};

/// Real idle hook installed by [`install`]. The argument is the
/// duration in ticks; the function programs SysTick and executes
/// `wfi`.
///
/// # Safety
///
/// Must be called in a context where reprogramming SysTick and waiting
/// for an interrupt cannot race with another owner of the timer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn caplet_cortex_m_idle(ticks: u32) {
    if ticks == 0 {
        return;
    }
    // Program the variable reload for the exact number of milliseconds
    // the kernel wants to sleep. The ISR advances the kernel clock by
    // the same clamped duration and disables the counter again, so the
    // time base stays one-shot between idle periods (真正的 tickless)。
    let _ = super::systick::reload_ms(ticks);
    super::systick::enable();
    // WFI — wait for the next interrupt. Returns immediately if an
    // interrupt is already pending.
    super::systick::wfi();
}

/// Idempotent install flag.
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Install the Cortex-M idle hook into the kernel. Safe to call
/// from `init_kernel` because the hook is only ever installed once.
pub fn install() {
    if INSTALLED.swap(true, Ordering::AcqRel) {
        return;
    }
    // Keep a real reference to the exception handler so the linker
    // pulls it out of the rlib; cortex-m-rt's vector table uses a weak
    // `PROVIDE(SysTick = DefaultHandler)` alias.
    #[cfg(target_arch = "arm")]
    let _handler: extern "C" fn() = super::systick::SysTick;
    // Tickless operation uses a one-shot SysTick. A continuously
    // running periodic counter would wake the core (and add time)
    // while tasks are executing, defeating the purpose of the idle
    // path and making the IRQ accounting non-deterministic.
    super::systick::disable();
    super::kernel_bridge::set_idle_hook(Some(caplet_cortex_m_idle));
}
