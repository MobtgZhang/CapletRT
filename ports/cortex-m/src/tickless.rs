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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn caplet_cortex_m_idle(ticks: u32) {
    if ticks == 0 {
        return;
    }
    // Reload SysTick. The current value is cleared as a side effect
    // of writing SYST_CVR.
    super::systick::reload(ticks);
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
    super::kernel_bridge::set_idle_hook(Some(caplet_cortex_m_idle));
}