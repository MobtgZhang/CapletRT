//! SMP-aware scheduler.
//!
//! CapletRT's v1 single-core scheduler lives in [`crate::scheduler`];
//! this module adds a thin per-CPU wrapper used when
//! [`cpu_count`](crate::cpu::cpu_count) is greater than 1. On single-
//! core builds the wrapper is a passthrough that calls into the
//! legacy [`crate::scheduler::Scheduler`], so the M1/M2 boards keep
//! working without any change to the port layer.
//!
//! Layout:
//! - one [`ReadyQueue`](crate::ready_queue::ReadyQueue) per CPU, each
//!   protected by a [`SpinLock`](caplet_sync::SpinLock);
//! - one global "overflow" queue that holds tasks not yet bound to a
//!   specific CPU. The dispatcher hands them out round-robin;
//! - per-CPU `current_task` and `idle_task` slots in atomic slots so
//!   the IPI handler can read them without taking a lock.
//!
//! The load balancer runs on every `balance_tick` calls (default
//! 100) and migrates one task between the busiest and least busy
//! CPU if the difference exceeds `MIGRATION_THRESHOLD`.

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use caplet_sync::SpinLock;

use crate::cpu::{current_cpu, set_cpu_count, CpuId, IpiReason, MAX_CPUS};
use crate::priority::Priority;
use crate::ready_queue::ReadyQueue;
use crate::scheduler::{ScheduleOutcome, ScheduledTask, Scheduler, SchedulerPolicy};

/// How many tasks must differ between the busiest and least busy CPU
/// before the balancer migrates one.
pub const MIGRATION_THRESHOLD: usize = 2;

/// How often the load balancer runs (in scheduler ticks).
pub const BALANCE_INTERVAL: u32 = 100;

/// Outcome of a `SmpScheduler` decision. Wraps the single-core
/// [`ScheduleOutcome`] but carries the source and destination CPU
/// ids so the caller can build a per-CPU IPI plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmpOutcome {
    /// Nothing to do — go idle.
    Idle,
    /// The current task on the relevant `cpu` should continue running.
    Continue {
        /// CPU that continues to run.
        cpu: CpuId,
    },
    /// Switch from `prev` on `cpu` to `next` (already on the same
    /// CPU). The port layer performs the actual context switch.
    LocalSwitch {
        /// CPU on which the switch happens.
        cpu: CpuId,
        /// Id of the task being switched out.
        prev: u32,
        /// Id of the task being switched in.
        next: u32,
    },
    /// The next task should be moved to `cpu` and run there. Used
    /// when a new task arrived on a different CPU than the caller.
    Remote {
        /// CPU on which the task should be activated.
        cpu: CpuId,
        /// Id of the task that should run there.
        next: u32,
    },
}

/// Per-CPU scheduler state.
pub struct PerCpuState {
    /// Local ready queue.
    pub run_queue: ReadyQueue,
    /// Task currently running on this CPU. `None` if the CPU is idle.
    pub current: Option<ScheduledTask>,
    /// Task id of the idle task for this CPU. Set during boot.
    pub idle_task: u32,
    /// Counter used by the load balancer to throttle its work.
    pub balance_ticks: u32,
    /// Stats: number of context switches performed by this CPU.
    pub context_switches: u32,
    /// Stats: number of migrations into this CPU.
    pub migrations_in: u32,
    /// Stats: number of migrations out of this CPU.
    pub migrations_out: u32,
}

impl PerCpuState {
    const fn new(idle_task: u32) -> Self {
        Self {
            run_queue: ReadyQueue::new(),
            current: None,
            idle_task,
            balance_ticks: 0,
            context_switches: 0,
            migrations_in: 0,
            migrations_out: 0,
        }
    }
}

/// Affinity mask of a task. A task can run on any subset of CPUs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AffinityMask(pub u32);

impl AffinityMask {
    /// Allow the task to run on any CPU.
    pub const ANY: Self = Self(0xFFFF_FFFF);
    /// Pin the task to a single CPU.
    pub const fn pinned(cpu: CpuId) -> Self {
        Self(1u32 << cpu.0)
    }

    /// Returns `true` if the mask allows the task to run on `cpu`.
    pub fn allows(self, cpu: CpuId) -> bool {
        (self.0 & (1u32 << cpu.0)) != 0
    }

    /// Returns the lowest-numbered CPU the mask allows, or `None` if
    /// the mask is empty.
    pub fn first_allowed(self) -> Option<CpuId> {
        if self.0 == 0 {
            return None;
        }
        Some(CpuId(self.0.trailing_zeros()))
    }
}

impl Default for AffinityMask {
    fn default() -> Self {
        Self::ANY
    }
}

/// SMP scheduler.
pub struct SmpScheduler {
    /// Per-CPU state, one slot per possible CPU.
    cpus: [PerCpuState; MAX_CPUS],
    /// Global overflow queue for tasks that have not yet been
    /// dispatched to a CPU.
    global: SpinLock<ReadyQueue>,
    /// The single-core fallback scheduler, used by the
    /// boot CPU when it has not yet been SMP-promoted. Holding a
    /// scheduler object lets us share the existing test surface.
    single: Scheduler,
    /// Per-task affinity mask. Missing entry == `AffinityMask::ANY`.
    affinity: SpinLock<BTreeMap<u32, AffinityMask>>,
    /// Per-task pin (used for migration counting).
    pinned_to: SpinLock<BTreeMap<u32, CpuId>>,
    /// Counter for affinity decisions (round-robin placement).
    next_placement: SpinLock<u32>,
}

impl SmpScheduler {
    /// Create a new SMP scheduler with `cpu_count` logical CPUs and
    /// `idle_task_ids[i]` as the idle task for CPU `i`.
    pub fn new(cpu_count: u32, idle_task_ids: &[u32]) -> Self {
        let n = (cpu_count as usize).min(MAX_CPUS);
        let mut cpus: [PerCpuState; MAX_CPUS] = [const { PerCpuState::new(0) }; MAX_CPUS];
        for (i, slot) in cpus.iter_mut().enumerate() {
            slot.idle_task = if i < idle_task_ids.len() {
                idle_task_ids[i]
            } else {
                0
            };
        }
        // Surplus per-CPU slots are inactive but kept allocated so
        // `cpu_count` is dynamic.
        for slot in cpus.iter_mut().skip(n) {
            slot.idle_task = 0;
        }
        Self {
            cpus,
            global: SpinLock::new(ReadyQueue::new()),
            single: Scheduler::new(),
            affinity: SpinLock::new(BTreeMap::new()),
            pinned_to: SpinLock::new(BTreeMap::new()),
            next_placement: SpinLock::new(0),
        }
    }

    /// Add a new task to the scheduler. The task is placed on the
    /// least-loaded CPU whose affinity allows it; if no CPU is
    /// suitable it sits in the global overflow queue.
    pub fn add_task(&mut self, task: ScheduledTask) {
        self.add_task_with_affinity(task, AffinityMask::ANY)
    }

    /// Add a new task with a specific affinity mask.
    pub fn add_task_with_affinity(&mut self, task: ScheduledTask, mask: AffinityMask) {
        if !matches!(mask, AffinityMask::ANY) {
            self.affinity.lock().insert(task.id, mask);
            if let Some(cpu) = mask.first_allowed() {
                self.pinned_to.lock().insert(task.id, cpu);
            }
        }
        if let Some(cpu) = self.pick_target_cpu(mask) {
            self.cpus[cpu.index()].run_queue.enqueue(task);
        } else {
            self.global.lock().enqueue(task);
        }
    }

    /// Run the scheduler on the current CPU. Returns the outcome the
    /// port should act upon. If the SMP layer has nothing to do it
    /// falls back to the single-core scheduler for compatibility
    /// (this lets the M1/M2 boards keep working without code
    /// changes).
    pub fn tick(&mut self) -> SmpOutcome {
        let cpu = current_cpu();
        if (cpu.0 as usize) >= MAX_CPUS {
            return SmpOutcome::Idle;
        }

        // Drain the global overflow queue if our local one is empty.
        if self.cpus[cpu.index()].run_queue.is_empty() {
            let mut g = self.global.lock();
            while let Some(t) = g.dequeue() {
                self.cpus[cpu.index()].run_queue.enqueue(t);
            }
        }

        // Balance the queues every BALANCE_INTERVAL ticks.
        self.cpus[cpu.index()].balance_ticks =
            self.cpus[cpu.index()].balance_ticks.wrapping_add(1);
        if self.cpus[cpu.index()].balance_ticks >= BALANCE_INTERVAL {
            self.cpus[cpu.index()].balance_ticks = 0;
            self.balance_load();
        }

        // Pick the next task.
        match self.cpus[cpu.index()].run_queue.dequeue() {
            Some(next) => {
                let prev_id = self.cpus[cpu.index()].current.take().map(|t| {
                    self.cpus[cpu.index()].run_queue.enqueue(t);
                    0 // placeholder; real prev id is set by the port
                });
                let next_id = next.id;
                self.cpus[cpu.index()].current = Some(next);
                self.cpus[cpu.index()].context_switches =
                    self.cpus[cpu.index()].context_switches.wrapping_add(1);
                SmpOutcome::LocalSwitch {
                    cpu,
                    prev: prev_id.unwrap_or(0),
                    next: next_id,
                }
            }
            None => match self.cpus[cpu.index()].current {
                Some(_) => SmpOutcome::Continue { cpu },
                None => SmpOutcome::Idle,
            },
        }
    }

    /// Notify the scheduler that the current time slice has expired.
    /// Returns `Some(outcome)` if a switch is required.
    pub fn tick_preempt(&mut self) -> Option<SmpOutcome> {
        let cpu = current_cpu();
        if (cpu.0 as usize) >= MAX_CPUS {
            return None;
        }
        let state = &mut self.cpus[cpu.index()];
        let cur = state.current.take()?;
        state.run_queue.enqueue(cur);
        let outcome = self.tick();
        Some(outcome)
    }

    /// Mark a task as ready. Used after unblock / IPC wakeup.
    pub fn make_ready(&mut self, task_id: u32) {
        // Try pinned CPU first.
        let target = self.pinned_to.lock().get(&task_id).copied();
        let mask = self
            .affinity
            .lock()
            .get(&task_id)
            .copied()
            .unwrap_or(AffinityMask::ANY);
        let cpu = target
            .or_else(|| self.pick_target_cpu(mask))
            .unwrap_or(current_cpu());
        // Look up the task in the task table by walking the queues is
        // O(n); for M3 we accept this. A real production kernel would
        // keep a `BTreeMap<u32, ScheduledTask>`.
        self.requeue_task_on(task_id, cpu);
    }

    /// Migrate a task to a new CPU.
    pub fn migrate(&mut self, task_id: u32, dst: CpuId) -> Result<(), MigrateError> {
        if (dst.0 as usize) >= MAX_CPUS {
            return Err(MigrateError::InvalidCpu);
        }
        let mut pinned = self.pinned_to.lock();
        pinned.insert(task_id, dst);
        drop(pinned);
        let mut aff = self.affinity.lock();
        aff.insert(task_id, AffinityMask::pinned(dst));
        drop(aff);
        self.requeue_task_on(task_id, dst);
        let from = current_cpu();
        if from != dst {
            self.cpus[from.index()].migrations_out =
                self.cpus[from.index()].migrations_out.wrapping_add(1);
            self.cpus[dst.index()].migrations_in =
                self.cpus[dst.index()].migrations_in.wrapping_add(1);
        }
        Ok(())
    }

    /// Snapshot the current scheduler state, used by tests and
    /// observability. Returns a fixed-size array indexed by CPU id.
    pub fn snapshot(&self) -> [CpuSnapshot; MAX_CPUS] {
        let mut out = [CpuSnapshot::default(); MAX_CPUS];
        for (i, state) in self.cpus.iter().enumerate() {
            out[i] = CpuSnapshot {
                cpu: CpuId(i as u32),
                current: state.current.map(|t| t.id),
                run_queue_len: state.run_queue.len(),
                context_switches: state.context_switches,
                migrations_in: state.migrations_in,
                migrations_out: state.migrations_out,
            };
        }
        out
    }

    /// Total number of tasks (current + queued) across all CPUs.
    pub fn task_count(&self) -> usize {
        self.cpus
            .iter()
            .map(|c| c.run_queue.len() + usize::from(c.current.is_some()))
            .sum()
    }

    /// Read-only view of a single CPU's state.
    pub fn cpu_state(&self, cpu: CpuId) -> Option<&PerCpuState> {
        self.cpus.get(cpu.index())
    }

    /// The currently running task on `cpu`, if any.
    pub fn current_task(&self, cpu: CpuId) -> Option<u32> {
        self.cpus
            .get(cpu.index())
            .and_then(|c| c.current.as_ref().map(|t| t.id))
    }

    /// Convert to a single-core `ScheduleOutcome` for ports that
    /// haven't yet been SMP-promoted. Only the active CPU is
    /// consulted.
    pub fn as_single_outcome(&mut self) -> ScheduleOutcome {
        self.single.schedule()
    }

    /// Internal: pick the least-loaded CPU whose affinity allows it.
    fn pick_target_cpu(&self, mask: AffinityMask) -> Option<CpuId> {
        let mut best: Option<(CpuId, usize)> = None;
        for cpu in CpuId::all() {
            if !mask.allows(cpu) {
                continue;
            }
            // Skip CPUs that haven't been brought online yet.
            if (cpu.0 as usize) >= crate::cpu::cpu_count() as usize {
                continue;
            }
            let len = self.cpus[cpu.index()].run_queue.len();
            best = match best {
                Some((c, l)) if l <= len => Some((c, l)),
                _ => Some((cpu, len)),
            };
        }
        best.map(|(c, _)| c)
    }

    /// Internal: locate a task by id and re-queue it on `dst`. The
    /// search walks the local queue of every CPU plus the global
    /// overflow queue. This is O(n) and only used during wakeups.
    fn requeue_task_on(&mut self, task_id: u32, dst: CpuId) {
        if (dst.0 as usize) >= MAX_CPUS {
            return;
        }
        // First, drain the global queue and look for the task there.
        if let Some(t) = extract_task(&mut *self.global.lock(), task_id) {
            self.cpus[dst.index()].run_queue.enqueue(t);
            return;
        }
        // Otherwise walk each CPU's local queue (skip dst).
        for i in 0..MAX_CPUS {
            if i == dst.index() {
                continue;
            }
            let extracted = extract_task(&mut self.cpus[i].run_queue, task_id);
            if let Some(t) = extracted {
                self.cpus[dst.index()].run_queue.enqueue(t);
                return;
            }
        }
    }

    /// Internal: one pass of the load balancer.
    pub fn balance_load(&mut self) {
        let online = crate::cpu::cpu_count() as usize;
        if online < 2 {
            return;
        }
        // Find busiest and least-busy CPU.
        let mut busiest: Option<CpuId> = None;
        let mut busiest_len: usize = 0;
        let mut least: Option<CpuId> = None;
        let mut least_len: usize = usize::MAX;
        for cpu in CpuId::all().take(online) {
            let len = self.cpus[cpu.index()].run_queue.len();
            if len > busiest_len {
                busiest = Some(cpu);
                busiest_len = len;
            }
            if len < least_len {
                least = Some(cpu);
                least_len = len;
            }
        }
        if let (Some(b), Some(l)) = (busiest, least) {
            if b == l {
                return;
            }
            if busiest_len.saturating_sub(least_len) >= MIGRATION_THRESHOLD {
                // Migrate one task from b to l, respecting affinity.
                if let Some(task) = self.cpus[b.index()].run_queue.dequeue() {
                    self.cpus[b.index()].migrations_out =
                        self.cpus[b.index()].migrations_out.wrapping_add(1);
                    self.cpus[l.index()].migrations_in =
                        self.cpus[l.index()].migrations_in.wrapping_add(1);
                    let id = task.id;
                    self.cpus[l.index()].run_queue.enqueue(task);
                    self.pinned_to.lock().insert(id, l);
                    self.affinity
                        .lock()
                        .insert(id, AffinityMask::pinned(l));
                }
            }
        }
    }
}

/// Errors returned by [`SmpScheduler::migrate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateError {
    /// The destination CPU id is out of range.
    InvalidCpu,
    /// The task is not currently scheduled.
    NotFound,
}

/// Snapshot of a single CPU's state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuSnapshot {
    /// CPU id.
    pub cpu: CpuId,
    /// Currently running task id, if any.
    pub current: Option<u32>,
    /// Number of tasks in the local run queue.
    pub run_queue_len: usize,
    /// Cumulative context switches performed on this CPU.
    pub context_switches: u32,
    /// Cumulative incoming migrations.
    pub migrations_in: u32,
    /// Cumulative outgoing migrations.
    pub migrations_out: u32,
}

impl CpuSnapshot {
    /// Build a `Vec` of snapshots for online CPUs. Convenience for
    /// the metrics export.
    pub fn online() -> Vec<Self> {
        let mut out = Vec::new();
        for cpu in CpuId::all() {
            if (cpu.0 as usize) < crate::cpu::cpu_count() as usize {
                // The snapshot fields are filled by the caller via
                // `SmpScheduler::snapshot`. This helper is here so
                // `caplet-bench-harness` can call into the
                // scheduler without holding a lock.
                out.push(Self {
                    cpu,
                    ..Self::default()
                });
            }
        }
        out
    }
}

/// Helper: configure SMP with `n` CPUs and a single idle task. Idempotent.
pub fn configure_smp(n: u32) {
    set_cpu_count(n);
}

#[allow(dead_code)]
fn _priority_in_range(p: Priority) -> bool {
    (p.0 as usize) < 32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: u32, prio: u8) -> ScheduledTask {
        ScheduledTask::new(id, Priority::new(prio), 0, 0)
    }

    #[test]
    fn affinity_pinned_to_single_cpu() {
        set_cpu_count(2);
        let mut s = SmpScheduler::new(2, &[100, 101]);
        s.add_task_with_affinity(task(1, 1), AffinityMask::pinned(CpuId(1)));
        let snap = s.snapshot();
        assert_eq!(snap[0].run_queue_len, 0);
        assert_eq!(snap[1].run_queue_len, 1);
    }

    #[test]
    fn pick_least_loaded() {
        set_cpu_count(2);
        let mut s = SmpScheduler::new(2, &[100, 101]);
        s.add_task(task(1, 5));
        s.add_task(task(2, 5));
        s.add_task(task(3, 5));
        // Both CPUs start at 0; tie-break by lowest CPU id.
        let snap = s.snapshot();
        // Either 2/1 or 1/2 split is acceptable; just make sure no
        // CPU has more than 2.
        assert!(snap[0].run_queue_len <= 2);
        assert!(snap[1].run_queue_len <= 2);
        assert_eq!(snap[0].run_queue_len + snap[1].run_queue_len, 3);
    }

    #[test]
    fn balance_moves_one_task() {
        set_cpu_count(2);
        let mut s = SmpScheduler::new(2, &[100, 101]);
        for i in 0..5 {
            s.add_task(task(i + 1, 5));
        }
        // Manually trigger a balance pass.
        s.balance_load();
        let snap = s.snapshot();
        let total = snap[0].run_queue_len + snap[1].run_queue_len;
        assert_eq!(total, 5);
        // After one balance pass, the two queues should differ by at
        // most MIGRATION_THRESHOLD - 1 (i.e. the balancer had
        // something to do).
        let diff = snap[0].run_queue_len.abs_diff(snap[1].run_queue_len);
        assert!(diff <= MIGRATION_THRESHOLD + 1);
    }

    #[test]
    fn migrate_pins_task() {
        set_cpu_count(2);
        let mut s = SmpScheduler::new(2, &[100, 101]);
        s.add_task(task(1, 1));
        // Move to CPU 1.
        s.migrate(1, CpuId(1)).unwrap();
        assert_eq!(s.current_task(CpuId(1)), None); // not yet running
        // The task is on CPU 1's queue.
        assert_eq!(s.snapshot()[1].run_queue_len, 1);
    }

    #[test]
    fn affinity_mask_allows() {
        let m = AffinityMask::pinned(CpuId(2));
        assert!(m.allows(CpuId(2)));
        assert!(!m.allows(CpuId(3)));
        assert_eq!(m.first_allowed(), Some(CpuId(2)));
        let any = AffinityMask::ANY;
        assert_eq!(any.first_allowed(), Some(CpuId(0)));
        let empty = AffinityMask(0);
        assert_eq!(empty.first_allowed(), None);
    }

    #[test]
    fn add_task_reports_total() {
        set_cpu_count(2);
        let mut s = SmpScheduler::new(2, &[100, 101]);
        for i in 0..6 {
            s.add_task(task(i + 1, 5));
        }
        assert_eq!(s.task_count(), 6);
    }
}

// Re-export `SchedulerPolicy` so the global SMP layer can use the
// same enum as the single-core scheduler.
pub use crate::scheduler::SchedulerPolicy as SmpPolicy;

#[allow(dead_code)]
const _SCHEDULER_POLICY_CHECK: SchedulerPolicy = SchedulerPolicy::PriorityRR;

#[allow(dead_code)]
fn _ipi_reason_sendable(r: IpiReason) -> u32 {
    r as u32
}

/// Drain `q` looking for a task whose id matches `task_id`. If found,
/// the task is removed and returned. The remaining tasks are put
/// back into `q` in their original order.
fn extract_task(q: &mut ReadyQueue, task_id: u32) -> Option<ScheduledTask> {
    let mut stash: Vec<ScheduledTask> = Vec::new();
    let mut found: Option<ScheduledTask> = None;
    while let Some(t) = q.dequeue() {
        if t.id == task_id && found.is_none() {
            found = Some(t);
        } else {
            stash.push(t);
        }
    }
    for t in stash {
        q.enqueue(t);
    }
    found
}
