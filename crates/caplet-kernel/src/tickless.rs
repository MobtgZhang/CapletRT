//! Tickless idle support for the kernel.
//!
//! The minimal QEMU / Cortex-M implementation uses the SysTick
//! countdown timer to schedule the next wakeup. When the scheduler
//! has no work to do it hands the next deadline to the port, which
//! loads the SysTick reload register and enters WFI. The SysTick
//! ISR calls [`tickless::on_tick`] to advance the kernel notion of
//! time and wake any tasks whose deadlines have elapsed.
//!
//! The implementation is intentionally tiny — Phase 5 expands it to
//! a full timer wheel. Right now the public API is just:
//!
//! * [`enter_idle`] — called by the kernel when no tasks are ready.
//! * [`on_tick`] — called by the SysTick ISR.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Monotonically increasing tick counter, incremented on every
/// SysTick interrupt. Backs all kernel timekeeping until Phase 5
/// brings in a timer wheel.
pub static TICKS: AtomicU32 = AtomicU32::new(0);

/// Read the current tick count.
pub fn now() -> u32 {
    TICKS.load(Ordering::Relaxed)
}

/// Advance the tick count by `delta`. Called by the SysTick ISR.
pub fn advance(delta: u32) {
    TICKS.fetch_add(delta, Ordering::Relaxed);
}

/// Advance the tick count by one. Convenience for periodic ticks
/// (1 kHz SysTick) and for unit tests.
pub fn on_tick() {
    advance(1);
}

/// Function-pointer type the port registers for the actual low-power
/// sleep. `ticks` is the duration the port should sleep for (the
/// kernel has already rounded to the nearest SysTick boundary).
pub type IdleFn = unsafe extern "C" fn(ticks: u32);

/// Installed idle hook. `core::ptr::null()` means "no-op".
static IDLE_HOOK: AtomicUsize = AtomicUsize::new(0);

/// Install the port's idle hook. Called once at boot. Passing `None`
/// resets to the no-op default.
pub fn set_idle_hook(hook: Option<IdleFn>) {
    let raw = hook.map(|f| f as usize).unwrap_or(0);
    IDLE_HOOK.store(raw, Ordering::Release);
}

/// Enter tickless idle for `ticks` ticks. The default (host) build
/// does nothing; the port registers a hook at boot that performs the
/// real WFI.
pub fn enter_idle(ticks: u32) {
    let raw = IDLE_HOOK.load(Ordering::Acquire);
    if raw == 0 {
        return;
    }
    let f: IdleFn = unsafe { core::mem::transmute(raw) };
    unsafe { f(ticks) }
}

/// Per-board deadline hint for the next SysTick reload. Read by the
/// port's WFI routine so it can program the SysTick timer to wake
/// exactly when the next task is due. A value of 0 means "no
/// pending deadline" — the port can sleep indefinitely.
pub static NEXT_DEADLINE_TICKS: AtomicU32 = AtomicU32::new(0);

/// Set the next deadline (in ticks since boot) the port should wake
/// the CPU at. Called by the kernel whenever a new task is woken by
/// a timer.
pub fn set_next_deadline(deadline_ticks: u32) {
    NEXT_DEADLINE_TICKS.store(deadline_ticks, Ordering::Release);
}

/// Reset the deadline hint. Called when the port wakes from WFI.
pub fn clear_next_deadline() {
    NEXT_DEADLINE_TICKS.store(0, Ordering::Release);
}

/// Diagnostics: how many times the idle task has been woken.
///
/// Reset to 0 by [`reset_wakeup_count`].
pub static WAKEUP_COUNT: AtomicU32 = AtomicU32::new(0);

/// Total accumulated ticks the CPU spent in idle. Diagnostics.
/// 64-bit value stored as a pair of 32-bit atomics so it
/// works on `thumbv7em-none-eabihf` targets that lack
/// `AtomicU64`.
pub static IDLE_TICKS_HI: AtomicU32 = AtomicU32::new(0);
/// Low half of `IDLE_TICKS`.
pub static IDLE_TICKS_LO: AtomicU32 = AtomicU32::new(0);

/// Record a wakeup from idle (called by the idle task).
pub fn record_wakeup(delta: u32) {
    WAKEUP_COUNT.fetch_add(1, Ordering::Relaxed);
    // 64-bit add via two halves. We don't bother with
    // carry propagation under contention — diagnostics only.
    let prev = IDLE_TICKS_LO.fetch_add(delta, Ordering::Relaxed);
    if prev.wrapping_add(delta) < prev {
        IDLE_TICKS_HI.fetch_add(1, Ordering::Relaxed);
    }
}

/// Reset all tickless diagnostics. Available for tests.
pub fn reset_wakeup_count() {
    WAKEUP_COUNT.store(0, Ordering::Relaxed);
    IDLE_TICKS_LO.store(0, Ordering::Relaxed);
    IDLE_TICKS_HI.store(0, Ordering::Relaxed);
}