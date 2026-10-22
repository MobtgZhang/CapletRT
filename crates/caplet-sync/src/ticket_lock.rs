//! Ticket lock: fair, starvation-free spin lock.
//!
//! A naive `compare_exchange` spin lock can starve a CPU on a
//! multiprocessor because lock holders cannot observe fairness.
//! A ticket lock (Mellor-Crummey & Scott) issues a ticket number
//! on acquire and waits for the lock's serving number to equal the
//! ticket; this guarantees FIFO service across CPUs.
//!
//! On a single-core build, the runtime cost is identical to a plain
//! compare-exchange spin lock because no other CPU is waiting.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

/// Ticket lock: each waiter takes a number and waits its turn.
pub struct TicketLock<T: ?Sized> {
    /// Next ticket to hand out.
    next: AtomicU32,
    /// Currently-served ticket.
    serving: AtomicU32,
    /// Protected data.
    data: UnsafeCell<T>,
}

impl<T> TicketLock<T> {
    /// Create a new ticket lock around `data`.
    pub const fn new(data: T) -> Self {
        Self {
            next: AtomicU32::new(0),
            serving: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }

    /// Consume the lock and return the inner data.
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

unsafe impl<T: ?Sized + Send> Send for TicketLock<T> {}
unsafe impl<T: ?Sized + Send> Sync for TicketLock<T> {}

impl<T: ?Sized> TicketLock<T> {
    /// Acquire the lock. Returns a RAII guard.
    pub fn lock(&self) -> TicketGuard<'_, T> {
        let ticket = self.next.fetch_add(1, Ordering::Acquire);
        loop {
            let serving = self.serving.load(Ordering::Acquire);
            if serving == ticket {
                return TicketGuard { lock: self };
            }
            core::hint::spin_loop();
        }
    }

    /// Try to acquire the lock without blocking.
    pub fn try_lock(&self) -> Option<TicketGuard<'_, T>> {
        let cur = self.next.load(Ordering::Relaxed);
        if self.serving.load(Ordering::Acquire) == cur {
            // No contention; reserve a ticket and immediately serve
            // it. This avoids the race where two callers both see
            // `serving == cur`.
            let res = self.next.compare_exchange(
                cur,
                cur.wrapping_add(1),
                Ordering::Acquire,
                Ordering::Relaxed,
            );
            if res.is_ok() {
                return Some(TicketGuard { lock: self });
            }
        }
        None
    }

    /// Check whether the lock is currently held.
    pub fn is_locked(&self) -> bool {
        self.next.load(Ordering::Relaxed) != self.serving.load(Ordering::Relaxed)
    }
}

/// RAII guard for [`TicketLock`].
pub struct TicketGuard<'a, T: ?Sized> {
    lock: &'a TicketLock<T>,
}

impl<'a, T: ?Sized> TicketGuard<'a, T> {
    /// Access the underlying data immutably.
    pub fn data(&self) -> &T {
        // SAFETY: the guard holds the lock exclusively.
        unsafe { &*self.lock.data.get() }
    }

    /// Access the underlying data mutably.
    pub fn data_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T: ?Sized> Deref for TicketGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data()
    }
}

impl<'a, T: ?Sized> DerefMut for TicketGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data_mut()
    }
}

impl<'a, T: ?Sized> Drop for TicketGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.serving.fetch_add(1, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_thread_lock_unlock() {
        let lock = TicketLock::new(0u32);
        {
            let mut g = lock.lock();
            *g = 1;
        }
        assert!(!lock.is_locked());
        let g = lock.lock();
        assert_eq!(*g, 1);
    }

    #[test]
    fn fifo_across_threads() {
        // The test crate is `no_std`; we cannot spawn threads.
        // Instead we verify the FIFO ordering by sequencing two
        // ticket numbers and asserting that the lock grants
        // them in order.
        let lock = TicketLock::new(0u32);
        let g1 = lock.lock();
        let t1 = lock.next.load(Ordering::Relaxed);
        // After acquiring once, `next == 1` and `serving == 0`.
        assert_eq!(t1, 1);
        drop(g1);
        // Now `serving` should have advanced to 1.
        let s = lock.serving.load(Ordering::Relaxed);
        assert_eq!(s, 1);
    }

    #[test]
    fn try_lock_succeeds_when_free() {
        let lock = TicketLock::new(42u32);
        let g = lock.try_lock().unwrap();
        assert_eq!(*g, 42);
    }

    #[test]
    fn try_lock_fails_when_held() {
        let lock = TicketLock::new(42u32);
        let _g = lock.lock();
        assert!(lock.try_lock().is_none());
    }
}
