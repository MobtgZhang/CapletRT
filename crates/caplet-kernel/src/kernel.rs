//! Kernel main struct
//!
//! The kernel is the heart of CapletRT. It owns the scheduler, the slot
//! allocator, and the boot task. The port layer is responsible for the
//! actual context switch; the kernel's job is to *decide* what to run.

use crate::bump_allocator::init_bump_allocator;
use crate::ipc_namespace::{IpcNamespace, NamespaceId, NamespaceTree};
use crate::kernel_config::KernelConfig;
use crate::memory::MemoryLayout;
use crate::process::{Process, ProcessId, ProcessState, ProcessTable};
use crate::resource_table::ResourceTable;
use crate::scheduler::{ScheduleOutcome, ScheduledTask, Scheduler};
use crate::slot_allocator::SlotAllocator;
use ::caplet_cap::CSpace;
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
extern crate alloc;
use alloc::boxed::Box;
use alloc::vec::Vec;
/// ID of the current task (used by syscall handlers to look up the
/// task's CSpace). Updated by the scheduler on every context switch.
pub static CURRENT_TASK_ID: AtomicUsize = AtomicUsize::new(0);

/// Upper bound for the dense task-id lookup index.  Task ids beyond this
/// fall back to scanning the process table, so the index cannot be used to
/// force an unbounded allocation from an out-of-range id.
const TASK_INDEX_LIMIT: usize = 4096;

/// Address of the scheduler's saved stack-pointer slot while a task is
/// running.  Cooperative ports use this to return to the scheduler
/// without knowing anything about the kernel's stack layout.
static SCHEDULER_SP_PTR: AtomicUsize = AtomicUsize::new(0);
/// Address of a task-local stack-pointer slot written by
/// [`yield_current_task`].  The scheduler inspects it after regaining
/// control and stores the value in the scheduler's task record.
static TASK_YIELD_SP_PTR: AtomicUsize = AtomicUsize::new(0);
/// Task id that performed the most recent cooperative yield.
static TASK_YIELD_ID: AtomicUsize = AtomicUsize::new(0);
/// Port-supplied cooperative context-switch routine.
static CONTEXT_SWITCH_FN: AtomicUsize = AtomicUsize::new(0);

/// Voluntarily give up the CPU from a running task.
///
/// The kernel's [`Kernel::start_with_context_switch`] loop publishes a
/// resumable scheduler context before every switch.  A task calls this
/// function to save its own context and jump back there; the scheduler
/// then records the saved stack pointer and selects the next task.
///
/// This is the portable cooperative-scheduling primitive used by the
/// RISC-V and AArch64 ports.  It is a no-op when the kernel has not
/// started a context-switching scheduler (for example in host tests).
pub fn yield_current_task() {
    let switch_addr = CONTEXT_SWITCH_FN.load(Ordering::Acquire);
    let scheduler_slot = SCHEDULER_SP_PTR.load(Ordering::Acquire);
    if switch_addr == 0 || scheduler_slot == 0 {
        core::hint::spin_loop();
        return;
    }

    // Snapshot the scheduler's saved stack pointer.  The port's switch
    // routine wrote it before resuming us, so this is a valid context.
    let scheduler_sp = unsafe { *(scheduler_slot as *const usize) };
    if scheduler_sp == 0 {
        return;
    }

    let mut task_sp: usize = 0;
    TASK_YIELD_ID.store(CURRENT_TASK_ID.load(Ordering::Acquire), Ordering::Release);
    TASK_YIELD_SP_PTR.store(core::ptr::addr_of_mut!(task_sp) as usize, Ordering::Release);

    // SAFETY: `switch_addr` was installed by
    // `start_with_context_switch` and has type `ContextSwitchFn`.  Both
    // pointers refer to live stack slots for the duration of the call.
    let switch: ContextSwitchFn = unsafe { core::mem::transmute(switch_addr) };
    // The port ABI treats `next` as the saved context address itself
    // (not a pointer to a local slot). `prev` remains a slot pointer
    // because the switch routine stores the outgoing frame there.
    unsafe { switch(&mut task_sp, scheduler_sp as *mut usize) };
    // When another task switches back to us we resume here.  Clear any
    // stale marker left by a nested/last yield on this task.
    TASK_YIELD_SP_PTR.store(0, Ordering::Release);
}

/// Run a closure with a mutable reference to the current task's CSpace.
///
/// Every process owns exactly one CSpace. Tasks that are not bound to a
/// process (the boot task and host-side syscall tests) use the kernel
/// CSpace. There is exactly one source of truth for process CSpaces:
/// `Process::cspace`.
pub fn with_current_cspace<F, R>(f: F) -> R
where
    F: FnOnce(&mut CSpace) -> R,
{
    if let Some(k) = unsafe { get_kernel() } {
        let task_id = CURRENT_TASK_ID.load(Ordering::Acquire);
        // Task id 0 is the pre-scheduler/boot context: the scheduler has not
        // published a runnable task yet (its boot task is id 1), and host
        // syscall tests deliberately leave the id at its initial value.  Such
        // callers always use the kernel CSpace, so avoid scanning the process
        // table for an owner that cannot exist.  Besides being cheaper on the
        // syscall path, this keeps bounded verifiers from carrying the whole
        // task-owner graph into every proof.
        if task_id == 0 {
            return f(&mut k.cspace);
        }
        if let Some(pid) = find_pid_for_task(task_id) {
            if let Some(process) = k.processes.get_mut(pid) {
                return f(&mut process.cspace);
            }
        }
        return f(&mut k.cspace);
    }
    // The kernel is not initialised yet (early boot / host smoke
    // tests). Use a lazily created fallback owned by a spin lock; no
    // `'static mut` aliasing is involved.
    static FALLBACK_CSPACE: SpinLock<Option<CSpace>> = SpinLock::new(None);
    let mut guard = FALLBACK_CSPACE.lock();
    let cspace = guard.get_or_insert_with(CSpace::new);
    f(cspace)
}

/// Look up the ProcessId currently running task `task_id` belongs to.
fn find_pid_for_task(task_id: usize) -> Option<ProcessId> {
    let k = unsafe { get_kernel()? };
    let task_id = u32::try_from(task_id).ok()?;
    // The scheduler uses task ids as its primary key.  Keep a bounded dense
    // index for the kernel's own ids so syscall entry does not have to scan
    // every process (and so bounded verifiers do not have to enumerate the
    // whole process table on each call).  The linear fallback below preserves
    // correctness if a port installs a task id outside the indexed range or
    // updates `Process::task_id` directly.
    if let Some(Some(pid)) = k.task_owners.get(task_id as usize) {
        if k.processes
            .get(*pid)
            .is_some_and(|process| process.task_id == Some(task_id))
        {
            return Some(*pid);
        }
    }
    if k.processes.is_empty() {
        return None;
    }
    k.processes
        .iter()
        .find_map(|(pid, process)| (process.task_id == Some(task_id)).then_some(*pid))
}

/// The process that owns the currently running task, if any.
pub fn current_pid() -> Option<ProcessId> {
    find_pid_for_task(CURRENT_TASK_ID.load(Ordering::Acquire))
}

/// Callback the port layer installs to perform the actual context switch.
pub type ContextSwitchFn = unsafe extern "C" fn(*mut usize, *mut usize);

/// The main kernel struct
pub struct Kernel {
    /// Kernel configuration
    config: KernelConfig,
    /// Memory layout
    memory: MemoryLayout,
    /// The scheduler
    scheduler: Scheduler,
    /// Slot allocator
    slots: SlotAllocator,
    /// The kernel/boot CSpace. Processes never alias this object; they
    /// own their CSpace in [`Process::cspace`].
    pub cspace: CSpace,
    /// Resource table: maps `ResourceId` -> `Arc<dyn Resource>`.
    pub resources: ResourceTable,
    /// Whether kernel is initialized
    initialized: bool,
    /// Boot task ID
    boot_task_id: u32,
    /// M3: process table (Phase 3).
    pub processes: ProcessTable,
    /// Dense task-id -> process index for syscall entry.  Entries outside
    /// `TASK_INDEX_LIMIT` fall back to the process table scan.
    task_owners: Vec<Option<ProcessId>>,
    /// Next process id to allocate.
    next_pid: u32,
    /// M3: namespace tree.
    pub namespaces: NamespaceTree,
    /// Dynamic module pool and loaded-module table.
    #[cfg(feature = "module-loader")]
    pub modules: crate::module::ModuleRegistry,
}

impl Kernel {
    /// Create a new kernel instance
    pub fn new(config: KernelConfig) -> Self {
        let table_capacity = config.max_tasks.saturating_add(1);
        Self {
            config,
            memory: MemoryLayout::default(),
            scheduler: Scheduler::new(),
            slots: SlotAllocator::new(256),
            cspace: CSpace::new(),
            resources: ResourceTable::new(),
            initialized: false,
            boot_task_id: 0,
            processes: ProcessTable::with_capacity(table_capacity),
            task_owners: Vec::with_capacity(table_capacity),
            next_pid: 1,
            namespaces: NamespaceTree::new(),
            #[cfg(feature = "module-loader")]
            modules: crate::module::ModuleRegistry::new(),
        }
    }

    /// Initialize the kernel
    pub fn init(&mut self) {
        if self.initialized {
            return;
        }

        // Initialize memory
        self.memory = self.config.memory;

        // Initialize bump allocator for boot
        // The early bump allocator owns the dedicated boot heap, not
        // the whole of RAM.  Using `ram_start` would hand out addresses
        // inside the kernel image on boards that link at the start of
        // DRAM (RV64 / AArch64 virt).
        init_bump_allocator(self.memory.heap_start as usize, self.memory.heap_size);

        // Initialize slot allocator
        let max_slots = self.config.max_tasks * 4 + 32;
        self.slots = SlotAllocator::new(max_slots);

        // Initialize scheduler
        self.scheduler = Scheduler::new();

        // Create boot task
        self.boot_task_id = 1;

        self.initialized = true;
    }

    /// Add a task to the scheduler
    pub fn add_task(&mut self, task: ScheduledTask) {
        self.scheduler.add_task(task);
    }

    /// Drive the scheduler once and return the outcome. The port layer
    /// is responsible for using the outcome to perform a real context
    /// switch; this function only makes the *decision*.
    pub fn tick(&mut self) -> ScheduleOutcome {
        if !self.initialized {
            panic!("Kernel not initialized");
        }
        self.scheduler.schedule()
    }

    /// Timer tick: drives preemptive scheduling. Returns Some(outcome)
    /// if the kernel decided to switch tasks.
    pub fn timer_tick(&mut self) -> Option<ScheduleOutcome> {
        if !self.initialized {
            return None;
        }
        self.scheduler.tick()
    }

    /// Start the scheduler. The caller (port) passes a context-switch
    /// function pointer; the kernel invokes it after picking the next
    /// task.
    ///
    /// On success this function never returns.
    pub fn start(&mut self) -> ! {
        if !self.initialized {
            panic!("Kernel not initialized");
        }

        // Without a port-supplied switch routine there is no safe way
        // to change stacks. The port must call
        // `start_with_context_switch`; `start` remains a WFI idle loop
        // for single-task boot paths.
        loop {
            core::hint::spin_loop();
        }
    }

    /// Start the scheduler and perform real context switches through
    /// `switch`.
    ///
    /// `switch(prev_sp, next_sp)` must save the caller's stack pointer
    /// through `prev_sp` and resume from the value stored in `next_sp`.
    /// The port installs this routine. `CURRENT_TASK_ID` is updated
    /// before every switch so syscall handlers always resolve the
    /// correct CSpace.
    ///
    /// On success this function never returns.
    pub fn start_with_context_switch(&mut self, switch: ContextSwitchFn) -> ! {
        if !self.initialized {
            panic!("Kernel not initialized");
        }
        CONTEXT_SWITCH_FN.store(switch as usize, Ordering::Release);
        let mut prev_sp: usize = 0;
        loop {
            // Publish the stack slot that `switch` will fill with a
            // resumable scheduler context.  A task can then call
            // `yield_current_task` without knowing this address.
            SCHEDULER_SP_PTR.store(core::ptr::addr_of_mut!(prev_sp) as usize, Ordering::Release);
            match self.scheduler.schedule() {
                ScheduleOutcome::Idle => core::hint::spin_loop(),
                ScheduleOutcome::Continue { current } => {
                    CURRENT_TASK_ID.store(current as usize, Ordering::Release);
                    let next_sp = self
                        .scheduler
                        .task(current)
                        .map(|task| task.context)
                        .unwrap_or(0);
                    if next_sp == 0 {
                        core::hint::spin_loop();
                        continue;
                    }
                    unsafe { switch(&mut prev_sp, next_sp as *mut usize) };
                    Self::absorb_yield(&mut self.scheduler);
                }
                ScheduleOutcome::Switch { prev, next } => {
                    let next_sp = self
                        .scheduler
                        .task(next)
                        .map(|task| task.context)
                        .unwrap_or(0);
                    if next_sp == 0 {
                        // A task without a prepared context cannot be
                        // resumed. Move it back to the ready queue so a
                        // later context preparation can run it.
                        self.scheduler.make_ready(next);
                        core::hint::spin_loop();
                        continue;
                    }
                    CURRENT_TASK_ID.store(next as usize, Ordering::Release);
                    let _ = prev;
                    unsafe { switch(&mut prev_sp, next_sp as *mut usize) };
                    // The switch returns when a task cooperatively
                    // yields.  Record where that task was suspended;
                    // `schedule()` re-enqueues it on the next round.
                    Self::absorb_yield(&mut self.scheduler);
                }
            }
        }
    }

    /// Consume the marker written by [`yield_current_task`], if any.
    fn absorb_yield(scheduler: &mut Scheduler) {
        let slot = TASK_YIELD_SP_PTR.swap(0, Ordering::AcqRel);
        if slot == 0 {
            return;
        }
        let id = TASK_YIELD_ID.swap(0, Ordering::AcqRel) as u32;
        // SAFETY: the slot points into the suspended task's stack and
        // remains valid until that task is resumed (which cannot happen
        // until the scheduler stores this value and selects the task).
        let sp = unsafe { *(slot as *const usize) };
        if sp != 0 {
            scheduler.set_task_context(id, sp);
        }
    }

    /// Get kernel configuration
    pub fn config(&self) -> &KernelConfig {
        &self.config
    }

    /// Get memory layout
    pub fn memory(&self) -> &MemoryLayout {
        &self.memory
    }

    /// Get a mutable reference to the scheduler
    pub fn scheduler(&mut self) -> &mut Scheduler {
        &mut self.scheduler
    }

    /// Get a reference to the scheduler
    pub fn scheduler_ref(&self) -> &Scheduler {
        &self.scheduler
    }

    /// Check if kernel is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get boot task ID
    pub fn boot_task_id(&self) -> u32 {
        self.boot_task_id
    }

    /// M3: create a new process and return its id.
    pub fn create_process(&mut self, mut process: Process) -> ProcessId {
        let pid = ProcessId(self.next_pid);
        self.next_pid = self.next_pid.wrapping_add(1);
        process.pid = pid;
        let task_id = process.task_id;
        self.processes.insert(pid, process);
        if let Some(task_id) = task_id {
            self.record_task_owner(pid, task_id);
        }
        pid
    }

    /// M3: terminate a process and reap its state.
    pub fn terminate_process(&mut self, pid: ProcessId) -> Result<(), &'static str> {
        if let Some(mut p) = self.processes.remove(pid) {
            if let Some(task_id) = p.task_id {
                self.clear_task_owner(pid, task_id);
            }
            p.terminate();
            Ok(())
        } else {
            Err("unknown pid")
        }
    }

    /// M3: look up a process by id.
    pub fn get_process(&self, pid: ProcessId) -> Option<&Process> {
        self.processes.get(pid)
    }

    /// M3: look up a process mutably.
    pub fn get_process_mut(&mut self, pid: ProcessId) -> Option<&mut Process> {
        self.processes.get_mut(pid)
    }

    /// Bind (or unbind) the scheduler task associated with a process.
    ///
    /// This is the owned way to update [`Process::task_id`]: it keeps the
    /// task-id lookup index and the process quota's task owner in sync.
    pub fn set_task_id(
        &mut self,
        pid: ProcessId,
        task_id: Option<u32>,
    ) -> Result<(), &'static str> {
        let previous = self.processes.get(pid).ok_or("unknown pid")?.task_id;
        if let Some(previous) = previous {
            self.clear_task_owner(pid, previous);
        }
        let process = self.processes.get_mut(pid).ok_or("unknown pid")?;
        process.task_id = task_id;
        if let Some(task_id) = task_id {
            process.quota.set_task_id(task_id);
            self.record_task_owner(pid, task_id);
        }
        Ok(())
    }

    fn record_task_owner(&mut self, pid: ProcessId, task_id: u32) {
        let index = task_id as usize;
        if index >= TASK_INDEX_LIMIT {
            return;
        }
        if self.task_owners.len() <= index {
            self.task_owners.resize(index + 1, None);
        }
        self.task_owners[index] = Some(pid);
    }

    fn clear_task_owner(&mut self, pid: ProcessId, task_id: u32) {
        if let Some(slot) = self.task_owners.get_mut(task_id as usize) {
            if *slot == Some(pid) {
                *slot = None;
            }
        }
    }

    /// M3: create a new IPC namespace and return its id.
    pub fn create_namespace(
        &mut self,
        name: alloc::string::String,
        parent: Option<NamespaceId>,
    ) -> NamespaceId {
        let id = self.namespaces.next_id();
        let ns = IpcNamespace::new(id, name, parent);
        self.namespaces.insert(ns);
        id
    }

    /// M3: get a mutable reference to a process's CSpace.
    pub fn process_cspace_mut(&mut self, pid: ProcessId) -> Option<&mut CSpace> {
        self.processes
            .get_mut(pid)
            .map(|process| &mut process.cspace)
    }

    /// Bind the scheduler task `task_id` to `pid` (and vice versa).
    pub fn bind_task(&mut self, pid: ProcessId, task_id: u32) -> Result<(), &'static str> {
        self.set_task_id(pid, Some(task_id))?;
        if let Some(task) = self.scheduler.task_mut(task_id) {
            task.owner_pid = Some(pid);
        }
        Ok(())
    }

    /// Add a scheduler task on behalf of a process and establish the
    /// task -> process mapping atomically.
    pub fn add_task_for_process(
        &mut self,
        pid: ProcessId,
        mut task: ScheduledTask,
    ) -> Result<u32, &'static str> {
        if !self.processes.contains_key(pid) {
            return Err("unknown pid");
        }
        task.owner_pid = Some(pid);
        let task_id = task.id;
        self.scheduler.add_task(task);
        self.set_task_id(pid, Some(task_id))?;
        Ok(task_id)
    }

    /// M3: assign a process to a different IPC namespace.
    pub fn process_join_namespace(
        &mut self,
        pid: ProcessId,
        ns: NamespaceId,
    ) -> Result<(), &'static str> {
        if let Some(p) = self.processes.get_mut(pid) {
            p.ipc_namespace = ns;
            Ok(())
        } else {
            Err("unknown pid")
        }
    }

    /// M3: count of processes in each lifecycle state.
    pub fn process_stats(&self) -> (usize, usize, usize, usize) {
        let mut created = 0;
        let mut active = 0;
        let mut suspended = 0;
        let mut terminated = 0;
        for p in self.processes.values() {
            match p.state {
                ProcessState::Created => created += 1,
                ProcessState::Active => active += 1,
                ProcessState::Suspended => suspended += 1,
                ProcessState::Terminated => terminated += 1,
            }
        }
        (created, active, suspended, terminated)
    }
}

/// Global kernel instance.
///
/// We publish the kernel through an `AtomicPtr<Kernel>` after boot so
/// interrupt handlers (SysTick, PendSV) can grab a raw pointer without
/// holding a `SpinLock` guard across the actual context switch. The
/// pointer is stable for the lifetime of the program because the
/// kernel is `Box::leak`'d at init time.
///
/// A `SpinLock<()>` ([`KERNEL_GUARD`]) is used to serialise init and
/// prevent two callers from racing to publish different pointers.
static KERNEL_PTR: AtomicPtr<Kernel> = AtomicPtr::new(core::ptr::null_mut());

/// Serialises `init_kernel` so we never publish two different pointers.
static KERNEL_GUARD: SpinLock<()> = SpinLock::new(());

/// Get the global kernel instance as a raw pointer.
///
/// # Safety
/// The pointer is valid for the lifetime of the program. Callers must
/// not hold it across an `init_kernel` call. Prefer [`with_kernel`]
/// for safe access.
pub unsafe fn get_kernel() -> Option<&'static mut Kernel> {
    let p = KERNEL_PTR.load(Ordering::Acquire);
    if p.is_null() {
        None
    } else {
        Some(unsafe { &mut *p })
    }
}

/// Run a closure with a mutable reference to the global kernel.
///
/// Returns `None` if the kernel has not been initialized.
pub fn with_kernel<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Kernel) -> R,
{
    let k = unsafe { get_kernel()? };
    Some(f(k))
}

/// Initialize the global kernel
///
/// # Safety
/// Must be called exactly once before any other kernel call.
pub unsafe fn init_kernel(config: KernelConfig) {
    let _guard = KERNEL_GUARD.lock();
    if !KERNEL_PTR.load(Ordering::Acquire).is_null() {
        // Already initialised — idempotent.
        return;
    }
    let mut kernel = Kernel::new(config);
    kernel.init();
    let leaked: &'static mut Kernel = Box::leak(Box::new(kernel));
    KERNEL_PTR.store(leaked, Ordering::Release);
}
