//! Scheduler implementation
//!
//! The scheduler is a static-priority queue with optional time-slice
//! round-robin at the same priority. It does not perform the actual
//! context switch — that lives in `ports::cortex_m::context`. The
//! scheduler just decides *which* task to run next and stores the
//! metadata the port will need.

use crate::priority::Priority;
use crate::ready_queue::ReadyQueue;
use alloc::collections::BTreeMap;

/// Task scheduled for execution
#[derive(Debug, Clone, Copy)]
pub struct ScheduledTask {
    /// Task ID
    pub id: u32,
    /// Task priority
    pub priority: Priority,
    /// Entry point address
    pub entry: usize,
    /// Stack pointer (lowest address — Cortex-M stacks grow down)
    pub stack_pointer: usize,
    /// Stack size in bytes (used for budget enforcement and canary)
    pub stack_size: u32,
    /// Context pointer
    pub context: usize,
    /// Number of times this task has been scheduled
    pub run_count: u32,
    /// Time-slice budget in ticks (0 = no time-slicing)
    pub time_slice: u32,
    /// Ticks consumed by the current slice so far
    pub time_slice_used: u32,
    /// Process that owns this task (`None` for kernel/idle tasks).
    pub owner_pid: Option<crate::process::ProcessId>,
}

impl ScheduledTask {
    /// Create a new scheduled task
    pub fn new(id: u32, priority: Priority, entry: usize, stack_pointer: usize) -> Self {
        Self {
            id,
            priority,
            entry,
            stack_pointer,
            stack_size: 0,
            context: 0,
            run_count: 0,
            time_slice: 0,
            time_slice_used: 0,
            owner_pid: None,
        }
    }

    /// Set the stack size (caller knows the layout).
    pub fn with_stack_size(mut self, stack_size: u32) -> Self {
        self.stack_size = stack_size;
        self
    }

    /// Create with explicit time-slice budget
    pub fn with_time_slice(
        id: u32,
        priority: Priority,
        entry: usize,
        stack_pointer: usize,
        stack_size: u32,
        time_slice: u32,
    ) -> Self {
        let mut t = Self::new(id, priority, entry, stack_pointer);
        t.stack_size = stack_size;
        t.time_slice = time_slice;
        t
    }
}

impl PartialEq for ScheduledTask {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ScheduledTask {}

impl PartialOrd for ScheduledTask {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledTask {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Higher priority (lower number) should be `Greater` so it
        // pops first from the max-heap. We invert the comparison.
        other
            .priority
            .0
            .cmp(&self.priority.0)
            .then(other.id.cmp(&self.id))
    }
}

/// Scheduler policy
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SchedulerPolicy {
    /// First in, first out at same priority
    Fifo,
    /// Time-sliced round robin at same priority
    RoundRobin,
    /// Priority-based with time slicing
    #[default]
    PriorityRR,
}

/// Result of a `schedule()` call: tells the caller what to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleOutcome {
    /// No task was running and there is no ready task — go to idle.
    Idle,
    /// A different task was chosen; perform a context switch.
    Switch {
        /// ID of the task that was running (0 if none).
        prev: u32,
        /// ID of the task we are switching to.
        next: u32,
    },
    /// The same task continues (it was at the head of the queue).
    Continue {
        /// ID of the task that continues to run.
        current: u32,
    },
}

/// Static priority scheduler
pub struct Scheduler {
    /// Ready queue
    ready_queue: ReadyQueue,
    /// Current running task
    current: Option<ScheduledTask>,
    /// Previous task (kept across `block_current` so that the next
    /// `schedule()` can report the correct `prev` even if the previous
    /// task has been removed from the ready queue).
    previous: Option<u32>,
    /// Scheduler policy
    policy: SchedulerPolicy,
    /// Task table
    tasks: BTreeMap<u32, ScheduledTask>,
    /// Number of context switches
    context_switches: u32,
    /// Number of voluntary yields
    yields: u32,
    /// Number of preemptions due to time-slice expiry
    preemptions: u32,
}

impl Scheduler {
    /// Create a new scheduler
    pub fn new() -> Self {
        Self::with_policy(SchedulerPolicy::PriorityRR)
    }

    /// Create with a specific policy
    pub fn with_policy(policy: SchedulerPolicy) -> Self {
        Self {
            ready_queue: ReadyQueue::new(),
            current: None,
            previous: None,
            policy,
            tasks: BTreeMap::new(),
            context_switches: 0,
            yields: 0,
            preemptions: 0,
        }
    }

    /// Add a task to the scheduler
    pub fn add_task(&mut self, task: ScheduledTask) {
        self.tasks.insert(task.id, task);
        self.ready_queue.enqueue(task);
    }

    /// Mark a task as ready (e.g. after unblock / spawn).
    pub fn make_ready(&mut self, id: u32) {
        if self.current.as_ref().map(|task| task.id) == Some(id)
            || self.ready_queue.iter_ids().contains(&id)
        {
            // A running or already-runnable task must not acquire a
            // second queue entry. Timer expiry and explicit wakeups can
            // legitimately race at the API boundary.
            return;
        }
        if let Some(t) = self.tasks.get(&id) {
            self.ready_queue.enqueue(*t);
        }
    }

    /// Block the current task: remove it from the ready queue, keep
    /// `current` set so the scheduler still knows who's running. The
    /// caller is responsible for not calling `schedule()` until the
    /// task is unblocked.
    pub fn block_current(&mut self) -> Option<u32> {
        if let Some(t) = self.current.take() {
            self.ready_queue.remove(t.id);
            self.previous = Some(t.id);
            Some(t.id)
        } else {
            None
        }
    }

    /// Terminate a task: remove it from the ready queue and the task table.
    pub fn terminate(&mut self, id: u32) -> Option<ScheduledTask> {
        self.ready_queue.remove(id);
        if Some(id) == self.current.map(|t| t.id) {
            self.previous = Some(id);
            self.current = None;
        }
        self.tasks.remove(&id)
    }

    /// Choose the next task to run.
    ///
    /// Corrected logic (vs the previous buggy version): the current task
    /// is *only* put back into the ready queue if it is still ready
    /// (which we represent by `current.is_some()` — the kernel marks it
    /// gone via `block_current`/`terminate` when it should not run again).
    /// Otherwise, we drop `current` and switch to the next.
    pub fn schedule(&mut self) -> ScheduleOutcome {
        let next = match self.ready_queue.dequeue() {
            Some(n) => n,
            None => {
                // No ready tasks. Keep current if we still have one,
                // else go idle.
                return match self.current {
                    Some(t) => ScheduleOutcome::Continue { current: t.id },
                    None => ScheduleOutcome::Idle,
                };
            }
        };

        let prev_id = match self.current.take() {
            Some(t) => {
                // The current task is still ready (it wasn't blocked or
                // terminated), so put it back in the ready queue.
                let id = t.id;
                self.ready_queue.enqueue(t);
                id
            }
            None => self.previous.take().unwrap_or(0),
        };
        self.current = Some(next);
        self.context_switches += 1;
        ScheduleOutcome::Switch {
            prev: prev_id,
            next: next.id,
        }
    }

    /// Tick the scheduler. Called on each timer interrupt.
    ///
    /// `RoundRobin`/`PriorityRR` policies decrement the current task's
    /// time-slice counter; on expiry they rotate it back to the tail of
    /// the queue and pick the next task. If the current task is at lower
    /// priority than the head of the ready queue, a preemption happens
    /// even if the time slice has not expired.
    pub fn tick(&mut self) -> Option<ScheduleOutcome> {
        if matches!(
            self.policy,
            SchedulerPolicy::RoundRobin | SchedulerPolicy::PriorityRR
        ) {
            if let Some(cur) = self.current.as_mut() {
                cur.time_slice_used = cur.time_slice_used.saturating_add(1);
                if cur.time_slice > 0 && cur.time_slice_used >= cur.time_slice {
                    // Time slice expired; rotate.
                    cur.time_slice_used = 0;
                    self.preemptions += 1;
                    return Some(self.schedule());
                }
            }
        }

        // Preemption check: if a higher-priority task is ready, switch.
        let current_prio = self.current.as_ref().map(|c| c.priority);
        let head_prio = self.ready_queue.peek().map(|t| t.priority);
        if let (Some(cur), Some(head)) = (current_prio, head_prio) {
            if head.0 < cur.0 {
                self.preemptions += 1;
                return Some(self.schedule());
            }
        }
        if self.current.is_none() && self.ready_queue.peek().is_some() {
            return Some(self.schedule());
        }
        None
    }

    /// Get the currently running task
    pub fn current_task(&self) -> Option<&ScheduledTask> {
        self.current.as_ref()
    }

    /// Look up a scheduled task by id.
    pub fn task(&self, id: u32) -> Option<&ScheduledTask> {
        self.tasks.get(&id)
    }

    /// Look up a scheduled task mutably.
    pub fn task_mut(&mut self, id: u32) -> Option<&mut ScheduledTask> {
        self.tasks.get_mut(&id)
    }

    /// Store the saved stack pointer after a context switch.
    pub fn set_task_context(&mut self, id: u32, context: usize) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.context = context;
        }
        if let Some(current) = self.current.as_mut() {
            if current.id == id {
                current.context = context;
            }
        }
    }

    /// Get the number of context switches
    pub fn context_switches(&self) -> u32 {
        self.context_switches
    }

    /// Get the number of voluntary yields
    pub fn yields(&self) -> u32 {
        self.yields
    }

    /// Get the number of preemptions
    pub fn preemptions(&self) -> u32 {
        self.preemptions
    }

    /// Get the number of tasks (ready + running)
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    /// Get the ready queue length
    pub fn ready_len(&self) -> usize {
        self.ready_queue.len()
    }

    /// Check if scheduler has no work
    pub fn is_empty(&self) -> bool {
        self.ready_queue.is_empty() && self.current.is_none()
    }

    /// Look up a task by ID
    pub fn get_task(&self, id: u32) -> Option<&ScheduledTask> {
        self.tasks.get(&id)
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: u32, prio: u8) -> ScheduledTask {
        ScheduledTask::new(id, Priority::new(prio), 0, 0)
    }

    #[test]
    fn schedule_picks_highest_priority() {
        let mut s = Scheduler::new();
        s.add_task(task(1, 5));
        s.add_task(task(2, 1));
        s.add_task(task(3, 3));
        match s.schedule() {
            ScheduleOutcome::Switch { prev, next } => {
                assert_eq!(prev, 0);
                assert_eq!(next, 2);
            }
            o => panic!("unexpected outcome: {:?}", o),
        }
    }

    #[test]
    fn same_task_is_reenqueued_not_double_run() {
        let mut s = Scheduler::new();
        s.add_task(task(1, 1));
        // First schedule switches to 1 (no prev).
        let _ = s.schedule();
        // After first schedule: current=1, ready_queue is empty.
        assert_eq!(s.ready_len(), 0);
        // Second schedule: queue is empty but we still have a current
        // task, so we Continue. The important property is that
        // the same task does not appear twice in the ready queue
        // (no phantom leak).
        match s.schedule() {
            ScheduleOutcome::Continue { current } => assert_eq!(current, 1),
            o => panic!("expected Continue, got {:?}", o),
        }
        assert_eq!(s.ready_len(), 0);
    }

    #[test]
    fn blocked_task_is_not_reenqueued() {
        let mut s = Scheduler::new();
        s.add_task(task(1, 1));
        s.add_task(task(2, 2));
        let _ = s.schedule(); // picks 1, prev=0
        s.block_current(); // 1 leaves the ready queue
        match s.schedule() {
            ScheduleOutcome::Switch { prev, next } => {
                assert_eq!(prev, 1);
                assert_eq!(next, 2);
            }
            o => panic!("unexpected: {:?}", o),
        }
    }

    #[test]
    fn idle_when_nothing_ready() {
        let mut s = Scheduler::new();
        assert_eq!(s.schedule(), ScheduleOutcome::Idle);
    }

    #[test]
    fn tick_rotates_on_time_slice_expiry() {
        let mut s = Scheduler::with_policy(SchedulerPolicy::RoundRobin);
        let mut t = task(1, 1);
        t.time_slice = 3;
        s.add_task(t);
        s.add_task(task(2, 1));
        let _ = s.schedule(); // picks 1
                              // Three ticks should expire the slice.
        s.tick();
        s.tick();
        match s.tick() {
            Some(ScheduleOutcome::Switch { next, .. }) => assert_eq!(next, 2),
            other => panic!("expected switch, got {:?}", other),
        }
    }

    #[test]
    fn higher_priority_task_preempts() {
        let mut s = Scheduler::new();
        s.add_task(task(1, 5));
        s.add_task(task(2, 5));
        let _ = s.schedule(); // picks 1
                              // Now enqueue a higher-priority task.
        s.add_task(task(3, 1));
        match s.tick() {
            Some(ScheduleOutcome::Switch { next, .. }) => assert_eq!(next, 3),
            other => panic!("expected preempt, got {:?}", other),
        }
    }
}
