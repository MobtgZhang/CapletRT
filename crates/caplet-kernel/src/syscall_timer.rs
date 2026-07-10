//! Timer syscall handlers
//!
//! Provides basic timer functionality for the kernel:
//! - TimerGet: get current tick count
//! - TimerSet: set a oneshot timer callback
//! - TimerCancel: cancel a pending timer

use crate::syscall::{Syscall, SyscallArgs, SyscallResult};
use crate::kernel::CURRENT_TASK_ID;
use core::sync::atomic::{AtomicU32, Ordering};

const ERR_INVALID_TIMER: u32 = 30;
const ERR_TIMER_NOT_FOUND: u32 = 31;
const ERR_TIMER_BUSY: u32 = 32;

/// System uptime in milliseconds (low 32 bits)
static SYSTEM_TICKS_LOW: AtomicU32 = AtomicU32::new(0);
/// System uptime in milliseconds (high 32 bits)
/// We use two 32-bit atomics for all targets to support 64-bit uptime
/// without requiring AtomicU64 (not available on thumbv6m/thumbv7em).
static SYSTEM_TICKS_HIGH: AtomicU32 = AtomicU32::new(0);

/// Next timer ID to allocate
static NEXT_TIMER_ID: AtomicU32 = AtomicU32::new(1);

/// Timer state
#[derive(Debug, Clone, Copy)]
pub struct Timer {
    /// Timer ID
    pub id: u32,
    /// Expiry tick (milliseconds)
    pub expires_at: u64,
    /// Target task ID (0 = any)
    pub target_task: u32,
    /// Whether this timer is active
    pub active: bool,
}

/// Get current system ticks (milliseconds since boot)
pub fn get_ticks() -> u64 {
    // Combine high and low parts
    let high = SYSTEM_TICKS_HIGH.load(Ordering::Acquire) as u64;
    let low = SYSTEM_TICKS_LOW.load(Ordering::Acquire) as u64;
    (high << 32) | low
}

/// Increment system ticks (called by SysTick interrupt)
pub fn increment_ticks(ms: u32) {
    let new_low = SYSTEM_TICKS_LOW.fetch_add(ms, Ordering::Release);
    // Check for overflow (carry to high)
    if new_low > (u32::MAX - ms) {
        SYSTEM_TICKS_HIGH.fetch_add(1, Ordering::Release);
    }
}

/// Handle timer-related syscalls
pub fn handle_timer_syscall(args: SyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::TimerGet => {
            // arg0 = pointer to store tick count (u64)
            let tick_ptr = args.arg0 as *mut u64;
            if !tick_ptr.is_null() {
                let ticks = get_ticks();
                unsafe { tick_ptr.write(ticks); }
            }
            SyscallResult::ok(get_ticks() as u32)
        }
        Syscall::TimerSet => {
            // arg0 = expiry ticks (u64, split across arg0 and arg1)
            // arg1 = target task id (u32 in arg2)
            // Returns timer id
            let expires_low = args.arg0 as u64;
            let expires_high = (args.arg1 as u64) << 32;
            let expires_at = expires_low | expires_high;
            let target_task = args.arg2;

            let timer_id = NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed);
            // TODO: store timer in global timer table
            // For now, just acknowledge the timer was "set"
            SyscallResult::ok(timer_id)
        }
        Syscall::TimerCancel => {
            // arg0 = timer id
            let timer_id = args.arg0;
            // TODO: look up timer in global table and cancel it
            // For now, just acknowledge
            if timer_id == 0 {
                SyscallResult::err(ERR_INVALID_TIMER)
            } else {
                SyscallResult::ok(0)
            }
        }
        _ => SyscallResult::err(1),
    }
}
