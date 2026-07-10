//! Main task creation
//!
//! Provides the constructors for the boot task and the idle task.
//! The idle task is the one that runs when no other task is ready
//! and is the only place that issues `wfi` / `wfe` on the target
//! hardware.

use crate::kernel::Kernel;
use crate::priority::Priority;
use crate::scheduler::ScheduledTask;
use crate::tickless;

/// Create the main user task
pub fn create_main_task(kernel: &Kernel) -> ScheduledTask {
    ScheduledTask::new(
        1, // Main task ID
        Priority::default(),
        0, // Entry point set by board
        0x2000_1000, // Stack pointer
    )
}

/// Create idle task
pub fn create_idle_task() -> ScheduledTask {
    ScheduledTask::new(
        0xFF, // Idle task ID
        Priority::LOWEST,
        idle_task as *const () as u32,
        0x2000_2000, // Stack pointer
    )
}

/// Idle task — runs when no other tasks are ready.
///
/// The default implementation busy-spins (suitable for the host
/// `cargo test` path). On Cortex-M boards the [`idle_task`]
/// implementation is replaced by a `cfg(target_arch = "arm")`-
/// guarded version in the `caplet-cortex-m` port that uses WFI.
#[cfg(not(target_arch = "arm"))]
pub extern "C" fn idle_task() -> ! {
    loop {
        // No WFI on host; just spin.
        core::hint::spin_loop();
    }
}

/// Idle task — runs when no other tasks are ready.
///
/// Calls into the port (via [`tickless::enter_idle`]) which on
/// Cortex-M programs the SysTick reload register with the next
/// deadline and then executes the `wfi` instruction. The board
/// (or its `link.x`) is expected to provide a `wfi` symbol in
/// `caplet_cortex_m::systick::wfi`.
#[cfg(target_arch = "arm")]
#[unsafe(no_mangle)]
pub extern "C" fn idle_task() -> ! {
    loop {
        // Program the next deadline and wait for an interrupt.
        let deadline = tickless::NEXT_DEADLINE_TICKS.load(core::sync::atomic::Ordering::Relaxed);
        let now = tickless::now();
        let delta: u32 = compute_idle_delta(deadline, now);
        // Bump the wakeup counter so the test harness can confirm
        // we entered and exited idle as expected.
        tickless::record_wakeup(delta);
        tickless::enter_idle(delta);
    }
}

/// Compute how many ticks the idle task should sleep for.
///
/// Inputs:
/// * `deadline` — absolute deadline in ticks (0 = none)
/// * `now` — current tick count
///
/// Returns a positive value at least 1 to guarantee forward
/// progress even if the deadline is already in the past or
/// unknown.
#[cfg_attr(not(target_arch = "arm"), allow(dead_code))]
pub fn compute_idle_delta(deadline: u32, now: u32) -> u32 {
    if deadline == 0 {
        // No upcoming deadline (or deadline already passed):
        // sleep for the full SysTick period. Returning 1 here
        // means the port wakes on every tick; the kernel can
        // install a longer period via [`set_next_deadline`]
        // before re-entering idle.
        return 1;
    }
    if deadline <= now {
        // Deadline already in the past: fire immediately.
        return 1;
    }
    deadline - now
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_deadline_returns_one_tick() {
        assert_eq!(compute_idle_delta(0, 100), 1);
    }

    #[test]
    fn past_deadline_returns_one_tick() {
        assert_eq!(compute_idle_delta(50, 100), 1);
    }

    #[test]
    fn future_deadline_returns_delta() {
        assert_eq!(compute_idle_delta(150, 100), 50);
    }
}