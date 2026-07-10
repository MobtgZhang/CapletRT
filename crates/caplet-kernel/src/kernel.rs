//! Kernel main struct
//!
//! The kernel is the heart of CapletRT. It owns the scheduler, the slot
//! allocator, and the boot task. The port layer is responsible for the
//! actual context switch; the kernel's job is to *decide* what to run.

use crate::kernel_config::KernelConfig;
use crate::resource_table::ResourceTable;
use crate::scheduler::{ScheduleOutcome, ScheduledTask, Scheduler};
use crate::memory::MemoryLayout;
use crate::bump_allocator::init_bump_allocator;
use crate::slot_allocator::SlotAllocator;
use crate::priority::Priority;
use crate::ipc_namespace::{IpcNamespace, NamespaceId, NamespaceTree};
use crate::process::{Process, ProcessId, ProcessState};
use ::caplet_cap::CSpace;
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::boxed::Box;
/// ID of the current task (used by syscall handlers to look up the
/// task's CSpace). Updated by the scheduler on every context switch.
pub static CURRENT_TASK_ID: AtomicUsize = AtomicUsize::new(0);

/// Run a closure with a mutable reference to the current task's CSpace.
///
/// In the minimal model we share a single global CSpace for all tasks
/// (Phase 4 swaps in a per-task map). The closure is run with the
/// kernel's `&mut CSpace` so syscall handlers can `allocate` / `revoke`
/// slots.
pub fn with_current_cspace<F, R>(f: F) -> R
where
    F: FnOnce(&mut CSpace) -> R,
{
    if let Some(k) = unsafe { get_kernel() } {
        let task_id = CURRENT_TASK_ID.load(Ordering::Acquire);
        // M3: per-process CSpace when available.
        if !k.processes.is_empty() {
            if let Some(pid) = find_pid_for_task(task_id) {
                if let Some(cspace) = k.process_cspaces.get_mut(&pid) {
                    return f(cspace);
                }
            }
        }
        return f(&mut k.cspace);
    }
    // Fallback: a lazily-initialised global CSpace. We leak a Box to
    // get a `'static` mutable reference, then keep its address in an
    // atomic so subsequent calls reach the same CSpace.
    let ptr = SHARED_CSPACE_PTR.load(Ordering::Acquire);
    if ptr == 0 {
        init_shared_cspace();
    }
    let ptr = SHARED_CSPACE_PTR.load(Ordering::Acquire) as *mut CSpace;
    unsafe { f(&mut *ptr) }
}

/// Look up the ProcessId currently running task `task_id` belongs to.
/// M3: this is a simple linear scan — fine for v1 where the process
/// table is small.
fn find_pid_for_task(task_id: usize) -> Option<ProcessId> {
    if let Some(k) = unsafe { get_kernel() } {
        // In M1/M2 minimal model, we use a single global CSpace
        // Return the first process if any exist, or use root namespace
        if !k.processes.is_empty() {
            // For minimal model, task_id maps to processes by index
            // In a full implementation, we'd maintain a task -> process mapping
            return k.processes.keys().next().copied();
        }
    }
    None
}

#[doc(hidden)]
pub fn with_current_cspace_into_static() -> Option<&'static mut CSpace> {
    // M3: leak a process's CSpace for the syscall_m3 helpers.
    if let Some(k) = unsafe { get_kernel() } {
        if let Some((_, cspace)) = k.process_cspaces.iter_mut().next() {
            let p: *mut CSpace = cspace;
            return Some(unsafe { &mut *p });
        }
    }
    None
}

/// Pointer (as usize) to a leak-allocated global CSpace used as a fallback.
static SHARED_CSPACE_PTR: AtomicUsize = AtomicUsize::new(0);

fn init_shared_cspace() {
    let boxed: &'static mut CSpace = Box::leak(Box::new(CSpace::new()));
    SHARED_CSPACE_PTR.store(boxed as *mut CSpace as usize, Ordering::Release);
}

/// Callback the port layer installs to perform the actual context switch.
pub type ContextSwitchFn = unsafe extern "C" fn(*mut u32, *mut u32);

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
    /// The kernel's working CSpace (single, shared across tasks in the
    /// minimal model; Phase 4 will introduce a per-task map).
    pub cspace: CSpace,
    /// Resource table: maps `ResourceId` -> `Arc<dyn Resource>`.
    pub resources: ResourceTable,
    /// Whether kernel is initialized
    initialized: bool,
    /// Boot task ID
    boot_task_id: u32,
    /// M3: process table (Phase 3).
    pub processes: BTreeMap<ProcessId, Process>,
    /// Next process id to allocate.
    next_pid: u32,
    /// M3: per-process PID -> CSpace cache (Phase 3).
    pub process_cspaces: BTreeMap<ProcessId, CSpace>,
    /// M3: namespace tree.
    pub namespaces: NamespaceTree,
}

impl Kernel {
    /// Create a new kernel instance
    pub fn new(config: KernelConfig) -> Self {
        Self {
            config,
            memory: MemoryLayout::default(),
            scheduler: Scheduler::new(),
            slots: SlotAllocator::new(256),
            cspace: CSpace::new(),
            resources: ResourceTable::new(),
            initialized: false,
            boot_task_id: 0,
            processes: BTreeMap::new(),
            next_pid: 1,
            process_cspaces: BTreeMap::new(),
            namespaces: NamespaceTree::new(),
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
        let ram_start = self.memory.ram_start as usize;
        let ram_size = self.memory.ram_size;
        init_bump_allocator(ram_start, ram_size);

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

        // Main scheduler loop. On a real board, after the first switch
        // we never come back here. On a host test (or single-threaded
        // build) this loop walks the scheduler decisions.
        loop {
            if self.scheduler.is_empty() {
                // No tasks ready. In a real build the port would now
                // WFI; we just spin here.
                continue;
            }
            match self.scheduler.schedule() {
                ScheduleOutcome::Idle => continue,
                ScheduleOutcome::Continue { .. } => continue,
                ScheduleOutcome::Switch { .. } => {
                    // In a real build: the port's switch routine is
                    // invoked here. We don't perform the actual asm
                    // switch from the kernel crate (the port owns it).
                    // The port calls `kernel.timer_tick()` and then
                    // invokes the switch on `Switch` outcomes.
                }
            }
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
        // Stash a private CSpace for this process (Phase 3).
        let proc_cspace = core::mem::replace(&mut process.cspace, CSpace::new());
        self.processes.insert(pid, process);
        self.process_cspaces.insert(pid, proc_cspace);
        pid
    }

    /// M3: terminate a process and reap its state.
    pub fn terminate_process(&mut self, pid: ProcessId) -> Result<(), &'static str> {
        if let Some(mut p) = self.processes.remove(&pid) {
            p.terminate();
            self.process_cspaces.remove(&pid);
            Ok(())
        } else {
            Err("unknown pid")
        }
    }

    /// M3: look up a process by id.
    pub fn get_process(&self, pid: ProcessId) -> Option<&Process> {
        self.processes.get(&pid)
    }

    /// M3: create a new IPC namespace and return its id.
    pub fn create_namespace(&mut self, name: alloc::string::String, parent: Option<NamespaceId>) -> NamespaceId {
        let id = self.namespaces.next_id();
        let ns = IpcNamespace::new(id, name, parent);
        self.namespaces.insert(ns);
        id
    }

    /// M3: get a mutable reference to a process's CSpace.
    pub fn process_cspace_mut(&mut self, pid: ProcessId) -> Option<&mut CSpace> {
        self.process_cspaces.get_mut(&pid)
    }

    /// M3: assign a process to a different IPC namespace.
    pub fn process_join_namespace(&mut self, pid: ProcessId, ns: NamespaceId) -> Result<(), &'static str> {
        if let Some(p) = self.processes.get_mut(&pid) {
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
