//! Timer syscall handlers
//!
//! Timer state is a real table keyed by timer id. Expiry is driven by
//! [`increment_ticks`], which is called from the platform tick
//! interrupt. Expired timers wake their target task through the
//! scheduler; no callback pointers are stored in the kernel.

extern crate alloc;

use crate::kernel::{with_kernel, CURRENT_TASK_ID};
use crate::syscall::{Syscall, SyscallArgs, SyscallResult};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use caplet_sync::SpinLock;
use caplet_timer::{TimerError, TimerId, TimerWheel};
use core::sync::atomic::{AtomicU32, Ordering};

const ERR_INVALID_TIMER: u32 = 30;
const ERR_TIMER_NOT_FOUND: u32 = 31;
const ERR_TIMER_BUSY: u32 = 32;

/// System uptime in milliseconds (low 32 bits).
static SYSTEM_TICKS_LOW: AtomicU32 = AtomicU32::new(0);
/// System uptime in milliseconds (high 32 bits).
///
/// Two 32-bit atomics keep the implementation usable on targets that
/// do not provide `AtomicU64` (thumbv7em).
static SYSTEM_TICKS_HIGH: AtomicU32 = AtomicU32::new(0);

/// Next timer id to allocate. Zero is never a valid timer id.
static NEXT_TIMER_ID: AtomicU32 = AtomicU32::new(1);

/// Insert a wakeup timer. The caller supplies an absolute deadline in
/// milliseconds and the task that should become runnable at expiry.
///
/// Returns the timer id, or `ERR_TIMER_BUSY` when the non-zero id
/// space has been exhausted.
pub(crate) fn schedule_wakeup(expires_at: u64, target_task: u32) -> Result<u32, u32> {
    let owner_task = CURRENT_TASK_ID.load(Ordering::Acquire) as u32;
    let timer_id = {
        let mut state = TIMERS.lock();
        let timer_id = allocate_timer_id(&state)?;
        let wheel_id = state
            .wheel
            .schedule_at(expires_at, u64::from(timer_id))
            .map_err(|error| match error {
                TimerError::WheelFull => ERR_TIMER_BUSY,
                _ => ERR_INVALID_TIMER,
            })?;
        state.timers.insert(
            timer_id,
            Timer {
                id: timer_id,
                expires_at,
                target_task,
                owner_task,
                wheel_id,
            },
        );
        timer_id
    };
    publish_deadline_hint();
    Ok(timer_id)
}

fn allocate_timer_id(state: &TimerState) -> Result<u32, u32> {
    // The ID is the timer-wheel token, so it must not collide with a
    // live timer even after the 32-bit counter wraps.
    for _ in 0..1024 {
        let id = NEXT_TIMER_ID.fetch_add(1, Ordering::AcqRel);
        if id != 0 && !state.timers.contains_key(&id) {
            return Ok(id);
        }
    }
    Err(ERR_TIMER_BUSY)
}

/// Timer state.
#[derive(Debug, Clone, Copy)]
pub struct Timer {
    /// Timer ID.
    pub id: u32,
    /// Absolute expiry tick in milliseconds.
    pub expires_at: u64,
    /// Task to wake at expiry.
    pub target_task: u32,
    /// Task that created the timer. Only the owner (or the kernel)
    /// may cancel it.
    pub owner_task: u32,
    /// Internal wheel handle used for O(1) cancellation.
    wheel_id: TimerId,
}

struct TimerState {
    wheel: TimerWheel,
    timers: BTreeMap<u32, Timer>,
}

/// Live timer table plus the hierarchical expiry wheel. Metadata is
/// kept in the map for ownership checks; expiration is driven by the
/// wheel, so a tick no longer scans every live timer.
static TIMERS: SpinLock<TimerState> = SpinLock::new(TimerState {
    wheel: TimerWheel::new(),
    timers: BTreeMap::new(),
});

/// Get current system ticks (milliseconds since boot).
pub fn get_ticks() -> u64 {
    let high = SYSTEM_TICKS_HIGH.load(Ordering::Acquire) as u64;
    let low = SYSTEM_TICKS_LOW.load(Ordering::Acquire) as u64;
    (high << 32) | low
}

/// Increment system ticks and expire due timers.
///
/// Called by the SysTick / machine-timer interrupt. Using `ms == 0` is
/// a legal way to run the expiry pass without advancing time.
pub fn increment_ticks(ms: u32) {
    if ms != 0 {
        let new_low = SYSTEM_TICKS_LOW.fetch_add(ms, Ordering::AcqRel);
        if u64::from(new_low) + u64::from(ms) > u64::from(u32::MAX) {
            SYSTEM_TICKS_HIGH.fetch_add(1, Ordering::Release);
        }
        crate::tickless::advance(ms);
    }

    let now = get_ticks();
    let mut wake: Vec<u32> = Vec::new();
    {
        let mut state = TIMERS.lock();
        for token in state.wheel.advance_to(now) {
            if let Some(timer) = state.timers.remove(&(token as u32)) {
                wake.push(timer.target_task);
            }
        }
    }
    publish_deadline_hint();

    // Run quota/time-slice accounting on every tick, including ticks
    // where no timer expired. Waking is deferred until after the
    // scheduler has observed the current tick so a newly-ready task is
    // visible to the next dispatch (or to a subsequent timer tick).
    with_kernel(|kernel| {
        let _ = kernel.timer_tick();
        for task in wake {
            if task != 0 {
                kernel.scheduler().make_ready(task);
            }
        }
    });
}

fn publish_deadline_hint() {
    let next = TIMERS.lock().wheel.next_deadline();
    match next {
        Some(deadline) => crate::tickless::set_next_deadline(deadline.min(u32::MAX as u64) as u32),
        None => crate::tickless::clear_next_deadline(),
    }
}

/// Absolute tick of the next pending timer, if any.
pub fn next_deadline_ticks() -> Option<u64> {
    TIMERS.lock().wheel.next_deadline()
}

/// Number of live timers (tests / observability).
pub fn active_timer_count() -> usize {
    TIMERS.lock().timers.len()
}

/// Validate a user pointer range. On bare-metal targets the range
/// must be inside a configured flash/RAM/heap region. Host unit tests
/// cannot use the board memory map, so they only enforce non-null and
/// overflow safety.
fn valid_user_range(addr: usize, len: usize) -> bool {
    if addr == 0 || len == 0 || addr.checked_add(len).is_none() {
        return false;
    }
    if !cfg!(target_os = "none") {
        return true;
    }
    with_kernel(|kernel| kernel.memory().contains_range(addr, len)).unwrap_or(false)
}

/// Handle timer-related syscalls.
pub fn handle_timer_syscall(args: SyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        Syscall::TimerGet => {
            // arg0 = pointer to store tick count (u64).
            let tick_ptr = args.arg0 as *mut u64;
            let addr = tick_ptr as usize;
            if tick_ptr.is_null()
                || addr % core::mem::align_of::<u64>() != 0
                || !valid_user_range(addr, core::mem::size_of::<u64>())
            {
                return SyscallResult::err(ERR_INVALID_TIMER);
            }
            let ticks = get_ticks();
            // Safety: the range was validated above. On native targets
            // this is the caller's responsibility as usual.
            unsafe { tick_ptr.write(ticks) };
            SyscallResult::ok(ticks as u32)
        }
        Syscall::TimerSet => {
            // arg0/arg1 = absolute expiry (low/high u32)
            // arg2 = target task id (0 = current task)
            let expires_at = u64::from(args.arg0) | (u64::from(args.arg1) << 32);
            let current = CURRENT_TASK_ID.load(Ordering::Acquire) as u32;
            let target_task = if args.arg2 == 0 { current } else { args.arg2 };
            match schedule_wakeup(expires_at, target_task) {
                Ok(timer_id) => SyscallResult::ok(timer_id),
                Err(error) => SyscallResult::err(error),
            }
        }
        Syscall::TimerCancel => {
            // arg0 = timer id.
            let timer_id = args.arg0;
            if timer_id == 0 {
                return SyscallResult::err(ERR_INVALID_TIMER);
            }
            let current = CURRENT_TASK_ID.load(Ordering::Acquire) as u32;
            let mut state = TIMERS.lock();
            match state.timers.get(&timer_id) {
                Some(timer) if current == 0 || timer.owner_task == current => {
                    let wheel_id = timer.wheel_id;
                    state.timers.remove(&timer_id);
                    let _ = state.wheel.cancel(wheel_id);
                    drop(state);
                    publish_deadline_hint();
                    SyscallResult::ok(0)
                }
                Some(_) => SyscallResult::err(ERR_INVALID_TIMER),
                None => SyscallResult::err(ERR_TIMER_NOT_FOUND),
            }
        }
        _ => SyscallResult::err(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The timer wheel and millisecond clock are process-global. The
    // default test harness runs unit tests concurrently, so timer tests
    // must not observe each other's pending deadlines or clock moves.
    static TEST_LOCK: SpinLock<()> = SpinLock::new(());

    #[test]
    fn set_and_cancel_timer() {
        let _serial = TEST_LOCK.lock();
        let id = handle_timer_syscall(SyscallArgs::new(Syscall::TimerSet.number(), 1_000, 0, 0, 0));
        assert!(id.success);
        let before = active_timer_count();
        assert!(before >= 1);
        let cancel = handle_timer_syscall(SyscallArgs::new(
            Syscall::TimerCancel.number(),
            id.value,
            0,
            0,
            0,
        ));
        assert!(cancel.success, "cancel: {cancel:?}");
        let again = handle_timer_syscall(SyscallArgs::new(
            Syscall::TimerCancel.number(),
            id.value,
            0,
            0,
            0,
        ));
        assert!(!again.success);
        assert_eq!(again.error, ERR_TIMER_NOT_FOUND);
    }

    #[test]
    fn expired_timer_is_removed() {
        let _serial = TEST_LOCK.lock();
        let before = active_timer_count();
        let now = get_ticks();
        let id = handle_timer_syscall(SyscallArgs::new(
            Syscall::TimerSet.number(),
            now as u32,
            (now >> 32) as u32,
            0,
            0,
        ));
        assert!(id.success);
        assert_eq!(active_timer_count(), before + 1);
        increment_ticks(0);
        assert_eq!(active_timer_count(), before);
    }

    #[test]
    fn zero_cancel_is_rejected() {
        let _serial = TEST_LOCK.lock();
        let r = handle_timer_syscall(SyscallArgs::new(Syscall::TimerCancel.number(), 0, 0, 0, 0));
        assert!(!r.success);
        assert_eq!(r.error, ERR_INVALID_TIMER);
    }

    #[test]
    fn null_timer_get_pointer_is_rejected() {
        let _serial = TEST_LOCK.lock();
        let r = handle_timer_syscall(SyscallArgs::new(Syscall::TimerGet.number(), 0, 0, 0, 0));
        assert!(!r.success);
        assert_eq!(r.error, ERR_INVALID_TIMER);
    }

    #[test]
    fn tick_overflow_carries_into_high_word() {
        let _serial = TEST_LOCK.lock();
        SYSTEM_TICKS_LOW.store(u32::MAX - 1, Ordering::Release);
        SYSTEM_TICKS_HIGH.store(7, Ordering::Release);
        increment_ticks(1);
        assert_eq!(SYSTEM_TICKS_LOW.load(Ordering::Acquire), u32::MAX);
        increment_ticks(1);
        assert_eq!(SYSTEM_TICKS_LOW.load(Ordering::Acquire), 0);
        assert_eq!(SYSTEM_TICKS_HIGH.load(Ordering::Acquire), 8);
    }
}
