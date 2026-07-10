//! Waker implementation for async tasks
//!
//! The waker closes the loop between a future and the executor's wake queue.
//! When the future calls `waker.wake()`, the waker pushes a `WakeEntry` into
//! the shared `WakeQueue` so the executor will poll it again.

extern crate alloc;

use crate::wake_queue::{WakeEntry, WakeQueue, SharedWakeQueue};
use crate::wake_source::WakeSource;
use alloc::sync::Arc;
use alloc::task::Wake;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{RawWaker, RawWakerVTable, Waker};

const WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(
    caplet_waker_clone,
    caplet_waker_wake,
    caplet_waker_wake_by_ref,
    caplet_waker_drop,
);

/// Waker for async tasks in CapletRT.
///
/// Internally holds an `Arc<CapletWakerInner>` so cloning a `Waker` keeps the
/// `WakeQueue` and the `TaskId` accessible without unsafe state.
#[derive(Debug, Clone)]
pub struct CapletWaker {
    inner: Arc<CapletWakerInner>,
}

#[derive(Debug)]
struct CapletWakerInner {
    /// The task this waker is for.
    task_id: u32,
    /// Priority of the task (carried into the wake entry).
    priority: u8,
    /// Shared wake queue — push targets here.
    queue: SharedWakeQueue,
    /// Whether the task has been woken since last observed.
    waked: AtomicBool,
}

impl CapletWaker {
    /// Create a waker bound to a specific task and queue.
    pub fn new(task_id: u32, priority: u8, queue: SharedWakeQueue) -> Self {
        Self {
            inner: Arc::new(CapletWakerInner {
                task_id,
                priority,
                queue,
                waked: AtomicBool::new(false),
            }),
        }
    }

    /// Create a waker that ignores wake calls (used for tests / internal polling).
    pub fn noop() -> Self {
        Self::new(0, u8::MAX, Arc::new(WakeQueue::new()))
    }

    /// Push a wake entry into the target queue.
    pub fn push_wake(&self, source: WakeSource) {
        self.inner.queue.push(WakeEntry::new(
            caplet_task::TaskId::new(self.inner.task_id),
            self.inner.priority,
            source,
        ));
        self.inner.waked.store(true, Ordering::Release);
    }

    /// Check if the waker has been woken (and clear it).
    pub fn check_and_clear(&self) -> bool {
        self.inner.waked.swap(false, Ordering::AcqRel)
    }

    /// Check if the waker has been woken (without clearing).
    pub fn is_woken(&self) -> bool {
        self.inner.waked.load(Ordering::Acquire)
    }

    /// Task ID this waker belongs to.
    pub fn task_id(&self) -> u32 {
        self.inner.task_id
    }

    /// Convert to a standard `Waker` (calls back into `push_wake`).
    pub fn into_waker(self) -> Waker {
        let raw = Arc::into_raw(self.inner) as *const ();
        unsafe { Waker::from_raw(RawWaker::new(raw, &WAKER_VTABLE)) }
    }
}

impl Default for CapletWaker {
    fn default() -> Self {
        Self::noop()
    }
}

impl Wake for CapletWaker {
    fn wake(self: Arc<Self>) {
        self.push_wake(WakeSource::Manual);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.push_wake(WakeSource::Manual);
    }
}

unsafe fn caplet_waker_clone(data: *const ()) -> RawWaker {
    let inner = Arc::from_raw(data as *const CapletWakerInner);
    let cloned = Arc::clone(&inner);
    core::mem::forget(inner);
    RawWaker::new(Arc::into_raw(cloned) as *const (), &WAKER_VTABLE)
}

unsafe fn caplet_waker_wake(data: *const ()) {
    let inner = Arc::from_raw(data as *const CapletWakerInner);
    inner.queue.push(WakeEntry::new(
        caplet_task::TaskId::new(inner.task_id),
        inner.priority,
        WakeSource::Manual,
    ));
    inner.waked.store(true, Ordering::Release);
}

unsafe fn caplet_waker_wake_by_ref(data: *const ()) {
    let inner = &*(data as *const CapletWakerInner);
    inner.queue.push(WakeEntry::new(
        caplet_task::TaskId::new(inner.task_id),
        inner.priority,
        WakeSource::Manual,
    ));
    inner.waked.store(true, Ordering::Release);
}

unsafe fn caplet_waker_drop(data: *const ()) {
    let _ = Arc::from_raw(data as *const CapletWakerInner);
}
