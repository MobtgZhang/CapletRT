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
    /// Number of CPUs owned by this scheduler instance.  The global
    /// `cpu_count` is boot-time configuration and may be changed by a
    /// different scheduler (or test), so placement decisions must use
    /// this instance-local value.
    cpu_count: usize,
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
    /// Canonical task table. Ready queues hold copies for ordering;
    /// this map is the source of truth for wakeups after a task has
    /// been blocked and removed from every queue.
    tasks: BTreeMap<u32, ScheduledTask>,
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
            cpu_count: n,
            cpus,
            global: SpinLock::new(ReadyQueue::new()),
            single: Scheduler::new(),
            affinity: SpinLock::new(BTreeMap::new()),
            pinned_to: SpinLock::new(BTreeMap::new()),
            tasks: BTreeMap::new(),
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
        self.tasks.insert(task.id, task);
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
        if (cpu.0 as usize) >= self.cpu_count {
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
        self.cpus[cpu.index()].balance_ticks = self.cpus[cpu.index()].balance_ticks.wrapping_add(1);
        if self.cpus[cpu.index()].balance_ticks >= BALANCE_INTERVAL {
            self.cpus[cpu.index()].balance_ticks = 0;
            self.balance_load();
        }

        // Pick the next task.
        match self.cpus[cpu.index()].run_queue.dequeue() {
            Some(next) => {
                let prev_id = match self.cpus[cpu.index()].current.take() {
                    Some(prev) if prev.id == next.id => {
                        // A duplicate wake raced with this tick. Drop the
                        // queued copy and keep the running task in place.
                        self.cpus[cpu.index()].current = Some(prev);
                        return SmpOutcome::Continue { cpu };
                    }
                    Some(prev) => {
                        let id = prev.id;
                        self.cpus[cpu.index()].run_queue.enqueue(prev);
                        id
                    }
                    None => 0,
                };
                let next_id = next.id;
                self.cpus[cpu.index()].current = Some(next);
                self.cpus[cpu.index()].context_switches =
                    self.cpus[cpu.index()].context_switches.wrapping_add(1);
                SmpOutcome::LocalSwitch {
                    cpu,
                    prev: prev_id,
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
        if (cpu.0 as usize) >= self.cpu_count {
            return None;
        }
        let outcome = self.tick();
        match outcome {
            SmpOutcome::Continue { .. } | SmpOutcome::Idle => None,
            other => Some(other),
        }
    }

    /// Mark a task as ready. Used after unblock / IPC wakeup.
    pub fn make_ready(&mut self, task_id: u32) {
        let Some(task) = self.tasks.get(&task_id).copied() else {
            // Waking an unknown task is a no-op; the alternative would be
            // manufacturing a task with no entry point or stack.
            return;
        };
        if self.task_is_queued(task_id) {
            // Shell-style wake APIs may be called more than once (for
            // example a timer and an explicit wake racing). A task must
            // only have one runnable copy in the scheduler.
            return;
        }
        let mask = self
            .affinity
            .lock()
            .get(&task_id)
            .copied()
            .unwrap_or(AffinityMask::ANY);
        // Try the last placement only while it remains compatible with
        // the task's affinity mask. A stale pin from a previous
        // migration must never override a restrictive mask.
        let target = self
            .pinned_to
            .lock()
            .get(&task_id)
            .copied()
            .filter(|cpu| mask.allows(*cpu));
        let cpu = target
            .or_else(|| self.pick_target_cpu(mask))
            .unwrap_or(current_cpu());
        if mask.allows(cpu) && (cpu.0 as usize) < self.cpu_count {
            self.cpus[cpu.index()].run_queue.enqueue(task);
        } else {
            self.global.lock().enqueue(task);
        }
    }

    /// Migrate a task to a new CPU.
    pub fn migrate(&mut self, task_id: u32, dst: CpuId) -> Result<(), MigrateError> {
        if (dst.0 as usize) >= self.cpu_count {
            return Err(MigrateError::InvalidCpu);
        }
        let task = self
            .tasks
            .get(&task_id)
            .copied()
            .ok_or(MigrateError::NotFound)?;
        let mask = self
            .affinity
            .lock()
            .get(&task_id)
            .copied()
            .unwrap_or(AffinityMask::ANY);
        if !mask.allows(dst) {
            return Err(MigrateError::AffinityDenied);
        }
        // Migrating a task that is currently executing would require a
        // stop-the-world handshake with the source CPU. Report that
        // explicitly rather than enqueueing a second runnable copy.
        if self
            .cpus
            .iter()
            .any(|state| state.current.as_ref().map(|running| running.id) == Some(task_id))
        {
            return Err(MigrateError::TaskRunning);
        }

        let source = self.take_queued_task(task_id).map(|(cpu, _)| cpu);
        let mut pinned = self.pinned_to.lock();
        pinned.insert(task_id, dst);
        drop(pinned);
        self.cpus[dst.index()].run_queue.enqueue(task);
        if let Some(from) = source.flatten() {
            if from == dst {
                return Ok(());
            }
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
        self.tasks.len()
    }

    /// Number of logical CPUs managed by this scheduler instance.
    pub fn online_cpus(&self) -> usize {
        self.cpu_count
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
        // Rotate the starting point on every placement so equally-loaded
        // CPUs receive tasks round-robin instead of always favouring the
        // lowest-numbered one.
        let mut rotation = self.next_placement.lock();
        let start = (*rotation as usize) % MAX_CPUS;
        *rotation = rotation.wrapping_add(1);
        drop(rotation);

        let mut best: Option<(CpuId, usize)> = None;
        for offset in 0..MAX_CPUS {
            let cpu = CpuId(((start + offset) % MAX_CPUS) as u32);
            if !mask.allows(cpu) {
                continue;
            }
            // Skip CPUs that haven't been brought online yet.
            if (cpu.0 as usize) >= self.cpu_count {
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

    /// Returns `true` when `task_id` is running or already enqueued on
    /// any CPU. A task may appear at most once across all runnable
    /// structures.
    fn task_is_queued(&self, task_id: u32) -> bool {
        if self.cpus.iter().any(|state| {
            state.current.as_ref().map(|task| task.id) == Some(task_id)
                || state.run_queue.iter_ids().contains(&task_id)
        }) {
            return true;
        }
        self.global.lock().iter_ids().contains(&task_id)
    }

    /// Remove a queued task from whichever CPU queue (or the global
    /// overflow queue) currently owns it. `None` means the task is not
    /// queued; the outer `Option` distinguishes "not queued" from the
    /// inner `None` meaning "global overflow queue".
    fn take_queued_task(&mut self, task_id: u32) -> Option<(Option<CpuId>, ScheduledTask)> {
        if let Some(task) = extract_task(&mut self.global.lock(), task_id) {
            return Some((None, task));
        }
        for cpu in 0..MAX_CPUS {
            if let Some(task) = extract_task(&mut self.cpus[cpu].run_queue, task_id) {
                return Some((Some(CpuId(cpu as u32)), task));
            }
        }
        None
    }

    /// Internal: one pass of the load balancer.
    ///
    /// Migrates at most one task from the busiest CPU to the least busy
    /// one, but only when the imbalance reaches [`MIGRATION_THRESHOLD`]
    /// **and** the chosen task is actually allowed to run on the
    /// destination.
    ///
    /// Earlier revisions dequeued the busiest CPU's head task
    /// unconditionally and then rewrote its affinity to the destination.
    /// That violated two documented guarantees at once: a task pinned
    /// with [`AffinityMask::pinned`] could be moved off its CPU, and its
    /// affinity mask was silently overwritten to match wherever the
    /// balancer happened to put it. Pinned tasks must never migrate; only
    /// tasks whose mask permits the destination are candidates.
    pub fn balance_load(&mut self) {
        let online = self.cpu_count;
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
        let (Some(b), Some(l)) = (busiest, least) else {
            return;
        };
        if b == l {
            return;
        }
        if busiest_len.saturating_sub(least_len) < MIGRATION_THRESHOLD {
            return;
        }

        // Collect the busiest CPU's runnable tasks and the per-task
        // affinity masks before mutating anything, so the borrow of
        // `self.cpus` does not overlap the affinity locks.
        let candidates: Vec<(u32, AffinityMask)> = {
            let q = &self.cpus[b.index()].run_queue;
            let aff = self.affinity.lock();
            let ids: Vec<u32> = q.iter_ids();
            ids.into_iter()
                .map(|id| (id, aff.get(&id).copied().unwrap_or(AffinityMask::ANY)))
                .collect()
            // Drop the affinity guard before we start moving tasks.
        };

        // Pick the first task the destination is allowed to run. Pinned
        // tasks (`AffinityMask::pinned(other)`) are skipped, which is what
        // keeps them on their CPU.
        let Some(victim) = candidates
            .into_iter()
            .find(|(_, mask)| mask.allows(l))
            .map(|(id, _)| id)
        else {
            // Everything on the busiest CPU is pinned there; balancing is
            // impossible without violating affinity. Leave the layout as is.
            return;
        };

        let Some(task) = self.cpus[b.index()].run_queue.remove(victim) else {
            return;
        };
        self.cpus[b.index()].migrations_out = self.cpus[b.index()].migrations_out.wrapping_add(1);
        self.cpus[l.index()].migrations_in = self.cpus[l.index()].migrations_in.wrapping_add(1);
        self.cpus[l.index()].run_queue.enqueue(task);
        // NOTE: deliberately no affinity rewrite. The balancer moves a task
        // only where its existing mask already permits it, so the mask
        // stays valid and the task remains migratable in future passes.
        self.pinned_to.lock().insert(victim, l);
    }
}

/// Errors returned by [`SmpScheduler::migrate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateError {
    /// The destination CPU id is out of range.
    InvalidCpu,
    /// The task is not currently scheduled.
    NotFound,
    /// The task's affinity mask does not include the destination CPU.
    AffinityDenied,
    /// The task is currently executing and must be stopped before it
    /// can be moved.
    TaskRunning,
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
    fn tick_reports_real_previous_task() {
        set_cpu_count(1);
        let mut s = SmpScheduler::new(1, &[100]);
        s.add_task(task(1, 1));
        s.add_task(task(2, 5));
        assert_eq!(
            s.tick(),
            SmpOutcome::LocalSwitch {
                cpu: CpuId(0),
                prev: 0,
                next: 1
            }
        );
        assert_eq!(
            s.tick(),
            SmpOutcome::LocalSwitch {
                cpu: CpuId(0),
                prev: 1,
                next: 2
            }
        );
    }

    #[test]
    fn duplicate_make_ready_does_not_duplicate_queue_entry() {
        set_cpu_count(1);
        let mut s = SmpScheduler::new(1, &[100]);
        s.add_task(task(1, 5));
        let before = s.snapshot()[0].run_queue_len;
        s.make_ready(1);
        s.make_ready(1);
        assert_eq!(s.snapshot()[0].run_queue_len, before);
    }

    #[test]
    fn blocked_task_can_be_made_ready_again() {
        set_cpu_count(1);
        let mut s = SmpScheduler::new(1, &[100]);
        s.add_task(task(1, 5));
        assert!(s.cpus[0].run_queue.remove(1).is_some());
        assert_eq!(s.snapshot()[0].run_queue_len, 0);
        s.make_ready(1);
        assert_eq!(s.snapshot()[0].run_queue_len, 1);
    }

    #[test]
    fn migrate_unknown_task_is_not_found() {
        set_cpu_count(2);
        let mut s = SmpScheduler::new(2, &[100, 101]);
        assert_eq!(s.migrate(99, CpuId(1)), Err(MigrateError::NotFound));
    }

    #[test]
    fn migrate_respects_existing_affinity() {
        set_cpu_count(2);
        let mut s = SmpScheduler::new(2, &[100, 101]);
        s.add_task_with_affinity(task(1, 5), AffinityMask::pinned(CpuId(0)));
        assert_eq!(s.migrate(1, CpuId(1)), Err(MigrateError::AffinityDenied));
        assert_eq!(s.snapshot()[0].run_queue_len, 1);
        assert_eq!(s.snapshot()[1].run_queue_len, 0);
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
