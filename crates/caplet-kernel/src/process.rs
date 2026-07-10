//! Process abstraction for M3.
//!
//! In M1/M2 the kernel exposed a single shared CSpace used by all
//! tasks. M3 introduces a real *process*: an isolated execution
//! context with its own CSpace, its own page table (on MMU
//! architectures), and its own IPC endpoints. The [`Process`] struct
//! is the kernel-side representation. User code never sees it
//! directly; it interacts through [`ProcessId`] handles and the
//! capability system.

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::string::String;

use caplet_budget::Budget as ResourceBudget;
use caplet_cap::CSpace;

use crate::ipc_namespace::NamespaceId;

/// Opaque process identifier. The kernel hands these out via
/// `cap_grant` for cross-process capability transfer and via
/// `pid()` in syscall results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessId(pub u32);

/// State of a process in the M3 lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Currently scheduled on some CPU.
    Active,
    /// Created but not yet scheduled.
    Created,
    /// Voluntarily suspended (waiting on IPC, sleep, etc.).
    Suspended,
    /// Terminated; will be reaped by the parent.
    Terminated,
}

/// Handle to a page table. The actual data lives in the architecture
/// port (see `ports/cortex-a/src/mmu.rs`). The kernel only stores
/// the index so the port can look it up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L1Handle(pub u32);

/// A process in the M3 model.
///
/// The struct is owned by the kernel's `processes` table. The kernel
/// guarantees exclusive access via the `KERNEL_PTR`'s spinlock; the
/// field types are all `Send + Sync` so a `&Process` can be passed
/// to syscall handlers without further locking.
pub struct Process {
    /// Globally unique process id.
    pub pid: ProcessId,
    /// Parent process (None for the kernel's init process).
    pub parent: Option<ProcessId>,
    /// Current lifecycle state.
    pub state: ProcessState,
    /// Private CSpace. In M1/M2 this was a single global; M3 gives
    /// each process its own.
    pub cspace: CSpace,
    /// Page table handle. `None` on architectures without an MMU
    /// (Cortex-M).
    pub page_table: Option<L1Handle>,
    /// IPC endpoints owned by this process. Endpoints are addressable
    /// by their symbolic name within the process's namespace.
    pub ipc_endpoints: BTreeMap<String, u32>,
    /// IPC namespace this process belongs to. Different namespaces
    /// have different endpoint sets and access rules.
    pub ipc_namespace: NamespaceId,
    /// Resource budget (memory, CPU quota, message quota).
    pub budget: ResourceBudget,
    /// Entry point address.
    pub entry: u32,
    /// Lowest address of the stack region (Cortex-M stacks grow down).
    pub stack_base: u32,
    /// Stack size in bytes.
    pub stack_size: u32,
    /// Affinity mask — restricts which CPUs may run this process.
    pub affinity_mask: u32,
    /// Human-readable name (for debugging).
    pub name: String,
}

impl Process {
    /// Construct a fresh process. Called by the loader / spawn path.
    pub fn new(
        pid: ProcessId,
        name: String,
        entry: u32,
        stack_base: u32,
        stack_size: u32,
    ) -> Self {
        Self {
            pid,
            parent: None,
            state: ProcessState::Created,
            cspace: CSpace::new(),
            page_table: None,
            ipc_endpoints: BTreeMap::new(),
            ipc_namespace: NamespaceId(0),
            budget: ResourceBudget::default(),
            entry,
            stack_base,
            stack_size,
            affinity_mask: 0xFFFF_FFFF,
            name,
        }
    }

    /// Mark the process as running.
    pub fn activate(&mut self) {
        self.state = ProcessState::Active;
    }

    /// Mark the process as suspended (e.g. waiting on IPC).
    pub fn suspend(&mut self) {
        self.state = ProcessState::Suspended;
    }

    /// Mark the process as terminated. The reaper is responsible for
    /// removing the process from the kernel's table.
    pub fn terminate(&mut self) {
        self.state = ProcessState::Terminated;
    }

    /// Returns `true` if the process is allowed to run on `cpu`.
    pub fn can_run_on(&self, cpu: u32) -> bool {
        (self.affinity_mask & (1u32 << cpu)) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_process_starts_created() {
        let p = Process::new(ProcessId(1), String::from("test"), 0, 0, 0);
        assert_eq!(p.state, ProcessState::Created);
        assert_eq!(p.pid, ProcessId(1));
    }

    #[test]
    fn activate_marks_active() {
        let mut p = Process::new(ProcessId(2), String::from("test2"), 0, 0, 0);
        p.activate();
        assert_eq!(p.state, ProcessState::Active);
    }

    #[test]
    fn terminate_marks_terminated() {
        let mut p = Process::new(ProcessId(3), String::from("test3"), 0, 0, 0);
        p.terminate();
        assert_eq!(p.state, ProcessState::Terminated);
    }

    #[test]
    fn affinity_mask_filters() {
        let mut p = Process::new(ProcessId(4), String::from("test4"), 0, 0, 0);
        p.affinity_mask = 0b0101;
        assert!(p.can_run_on(0));
        assert!(!p.can_run_on(1));
        assert!(p.can_run_on(2));
        assert!(!p.can_run_on(3));
    }
}
