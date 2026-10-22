//! Async executor
//!
//! Drives stackless futures by polling them and re-enqueueing via the waker
//! when they yield. The executor owns one `WakeQueue` (shared via `Arc`
//! with all wakers) and a map of tasks. The tasks map is `Arc<SpinLock<...>>`
//! so the `Spawner` can spawn from any context that holds the `Arc<Executor>`.

extern crate caplet_task;
use crate::async_task::AsyncTask;
use crate::wake_queue::{SharedWakeQueue, WakeEntry, WakeQueue};
use crate::wake_source::WakeSource;
use crate::waker::CapletWaker;
use ::caplet_sync::SpinLock;
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::task::Context;

/// Future stored in the executor
type BoxedFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

/// The async executor
///
/// The inner state lives behind an `Arc<SpinLock<...>>` so the executor can
/// be shared via `Arc<Executor>` (e.g. for the `Spawner`) while still being
/// safely mutated from the kernel's single thread of execution.
pub struct Executor {
    inner: Arc<SpinLock<ExecutorInner>>,
}

struct ExecutorInner {
    /// All tasks indexed by ID
    tasks: BTreeMap<u32, ExecutorTask>,
    /// Queue of tasks ready to run (shared with all wakers)
    wake_queue: SharedWakeQueue,
    /// Currently running task ID
    current_task: Option<u32>,
    /// Next task ID to assign
    next_task_id: AtomicUsize,
    /// Executor state
    state: ExecutorState,
}

struct ExecutorTask {
    /// Task ID
    id: u32,
    /// The future
    future: BoxedFuture,
    /// Priority
    priority: u8,
    /// Stack size requirement
    stack_size: usize,
}

/// Public metadata snapshot for one executor task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskInfo {
    /// Task identifier assigned by [`Executor::spawn`].
    pub id: u32,
    /// Scheduling priority (lower is higher priority).
    pub priority: u8,
    /// Stack reservation requested at spawn time, in bytes.
    pub stack_size: usize,
}

/// Executor state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorState {
    /// Executor is not running
    Idle,
    /// Executor is running
    Running,
    /// Executor has finished
    Finished,
}

impl Executor {
    /// Create a new executor
    pub fn new() -> Self {
        Self {
            inner: Arc::new(SpinLock::new(ExecutorInner {
                tasks: BTreeMap::new(),
                wake_queue: Arc::new(WakeQueue::new()),
                current_task: None,
                next_task_id: AtomicUsize::new(1),
                state: ExecutorState::Idle,
            })),
        }
    }

    /// Get a clone of the wake queue (used by external code that wants to
    /// build wakers pointing into this executor).
    pub fn shared_queue(&self) -> SharedWakeQueue {
        Arc::clone(&self.inner.lock().wake_queue)
    }

    /// Spawn a task.
    pub fn spawn<F>(&self, future: F) -> u32
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_with(future, 16, 2048)
    }

    /// Spawn a task with explicit priority and stack.
    pub fn spawn_with<F>(&self, future: F, priority: u8, stack_size: usize) -> u32
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let id = self
            .inner
            .lock()
            .next_task_id
            .fetch_add(1, Ordering::Relaxed) as u32;
        let future: BoxedFuture = Box::pin(future);
        let mut guard = self.inner.lock();
        guard.tasks.insert(
            id,
            ExecutorTask {
                id,
                future,
                priority,
                stack_size,
            },
        );
        guard.wake_queue.push(WakeEntry::new(
            caplet_task::TaskId::new(id),
            priority,
            WakeSource::Initial,
        ));
        id
    }

    /// Spawn a task with explicit priority. Convenience for callers that
    /// don't need to set a stack size.
    pub fn spawn_with_priority<F>(&self, future: F, priority: u8) -> u32
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_with(future, priority, 4096)
    }

    /// Spawn an `AsyncTask` whose future, priority, stack are pre-built.
    pub fn spawn_async_task(&self, async_task: AsyncTask) -> u32 {
        let (future, priority, stack_size) = async_task.dissolve();
        self.spawn_with_dyn(future, priority, stack_size)
    }

    fn spawn_with_dyn(&self, future: BoxedFuture, priority: u8, stack_size: usize) -> u32 {
        let id = self
            .inner
            .lock()
            .next_task_id
            .fetch_add(1, Ordering::Relaxed) as u32;
        let mut guard = self.inner.lock();
        guard.tasks.insert(
            id,
            ExecutorTask {
                id,
                future,
                priority,
                stack_size,
            },
        );
        guard.wake_queue.push(WakeEntry::new(
            caplet_task::TaskId::new(id),
            priority,
            WakeSource::Initial,
        ));
        id
    }

    /// Run one scheduling cycle.
    ///
    /// A cycle polls a snapshot of the tasks that were ready when the cycle
    /// started.  Wakes raised while a task is being polled are queued for the
    /// next cycle.  This distinction matters for `yield_now()`: a high
    /// priority task that immediately wakes itself is polled once per cycle,
    /// so lower priority tasks cannot be starved by an ever-growing heap of
    /// self-wake entries.
    pub fn run_once(&self) -> bool {
        let queue = self.shared_queue();
        let snapshot = queue.take_ready_snapshot();
        if snapshot.is_empty() {
            self.inner.lock().state = ExecutorState::Idle;
            return false;
        }

        // A task may have received several wakes before the cycle started.
        // Poll it at most once this cycle; later wakes remain in the queue for
        // the next cycle.
        let mut seen = BTreeSet::new();
        let mut polled = false;
        for entry in snapshot {
            if !seen.insert(entry.task_id.as_u32()) {
                continue;
            }
            if self.poll_task(entry.task_id.as_u32()) {
                polled = true;
            }
        }

        polled
    }

    fn poll_task(&self, task_id: u32) -> bool {
        // Never hold the executor lock across user code.  A future is allowed
        // to spawn another task or call `Executor::wake`, both of which take
        // this lock.  Removing the task for the duration of the poll also
        // makes re-entrant executor calls safe.
        let mut task = {
            let mut inner = self.inner.lock();
            inner.state = ExecutorState::Running;
            inner.current_task = Some(task_id);
            match inner.tasks.remove(&task_id) {
                Some(task) => task,
                None => {
                    inner.current_task = None;
                    return false;
                }
            }
        };

        let queue = self.shared_queue();
        let waker = CapletWaker::new(task_id, task.priority, queue).into_waker();
        let mut cx = Context::from_waker(&waker);
        let poll_result = task.future.as_mut().poll(&mut cx);
        drop(waker);

        let mut inner = self.inner.lock();
        if poll_result.is_pending() {
            inner.tasks.insert(task_id, task);
        }
        inner.current_task = None;
        true
    }

    /// Run all currently-ready tasks once and return.
    pub fn run_until_idle(&self) {
        while self.run_once() {
            // keep going while there is something to poll
        }
    }

    /// Run until all tasks complete.
    ///
    /// Note: this is single-threaded. Tasks must wake themselves or be
    /// woken by other tasks for this to terminate.
    ///
    /// If there are blocked tasks waiting, the executor will spin-wait for
    /// them to be woken. In a full kernel integration, this would use
    /// WFI (Wait For Interrupt) or yield to the scheduler.
    pub fn run(&self) {
        let mut guard = self.inner.lock();
        guard.state = ExecutorState::Running;
        drop(guard);

        loop {
            if !self.run_once() {
                // No more ready tasks to poll
                let inner = self.inner.lock();

                if inner.tasks.is_empty() {
                    // All tasks have completed
                    break;
                }

                // Tasks remain but are blocked (waiting for wake)
                // In a full implementation, we would:
                // 1. Enter tickless idle (WFI on ARM)
                // 2. Or yield to the kernel scheduler
                // For now, we spin-wait for external wakers
                drop(inner);

                // Brief spin to allow wakers to queue tasks
                // In production, this should integrate with the kernel
                #[cfg(not(target_arch = "wasm32"))]
                {
                    core::hint::spin_loop();
                }
            }
        }

        self.inner.lock().state = ExecutorState::Finished;
    }

    /// Wake a task by ID
    pub fn wake(&self, task_id: u32) {
        let guard = self.inner.lock();
        if let Some(task) = guard.tasks.get(&task_id) {
            guard.wake_queue.push(WakeEntry::new(
                caplet_task::TaskId::new(task_id),
                task.priority,
                WakeSource::Manual,
            ));
        }
    }

    /// Get number of tasks
    pub fn task_count(&self) -> usize {
        self.inner.lock().tasks.len()
    }

    /// Look up metadata for a live task.
    pub fn task_info(&self, task_id: u32) -> Option<TaskInfo> {
        let inner = self.inner.lock();
        inner.tasks.get(&task_id).map(|task| TaskInfo {
            id: task.id,
            priority: task.priority,
            stack_size: task.stack_size,
        })
    }

    /// Get current executor state
    pub fn state(&self) -> ExecutorState {
        self.inner.lock().state
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}
