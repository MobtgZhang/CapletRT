//! Static and explicit task registration.
//!
//! `#[caplet::task]` emits a [`TaskRegistration`] for every task. The
//! registration can be collected in two ways:
//!
//! * **link-time** — tasks declared with `#[task(..., linkme)]` are placed
//!   in [`LINKED_TASKS`] by the linker; [`TaskTable::register_linked`]
//!   copies them into a bounded table without allocation.
//! * **explicit fallback** — pass the macro-generated registration to
//!   [`TaskTable::register`] from `main`/`kernel_init`. This is the
//!   portable path for toolchains or linker scripts where placing a
//!   custom section before the vector table is unsafe.
//!
//! Tables are fixed-capacity and `const`-constructible, so registration
//! works in a bare-metal boot path before the heap exists.

/// Description of a task for kernel introspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskDescriptor {
    /// Task name.
    pub name: &'static str,
    /// Priority (0..=31).
    pub priority: u8,
    /// Stack size in bytes.
    pub stack: usize,
    /// Optional spawn pool.
    pub pool: Option<&'static str>,
    /// Capability names requested by the task.
    pub caps: &'static [&'static str],
    /// CPU budget per epoch in microseconds.
    pub cpu_quota: u32,
    /// Heap memory budget in bytes.
    pub mem_budget: usize,
    /// Message budget per second.
    pub message_quota: u32,
}

impl TaskDescriptor {
    /// The entry passes the same basic descriptor validation as the
    /// kernel: priorities are five bits and stacks must be non-zero.
    pub const fn is_valid(&self) -> bool {
        self.priority <= 31 && self.stack > 0 && self.cpu_quota > 0
    }
}

/// A task entry emitted by `#[caplet::task]`.
///
/// `start` is present only for parameterless tasks. Tasks with
/// arguments still appear in the table for introspection, but their
/// spawn helper must be called explicitly with the arguments.
#[derive(Debug, Clone, Copy)]
pub struct TaskRegistration {
    /// Static task metadata.
    pub descriptor: &'static TaskDescriptor,
    /// Runtime launcher, when the task takes no arguments.
    pub start: Option<fn()>,
}

impl TaskRegistration {
    /// Create a registration for a task that can be launched without
    /// arguments.
    pub const fn new(descriptor: &'static TaskDescriptor, start: fn()) -> Self {
        Self {
            descriptor,
            start: Some(start),
        }
    }

    /// Create an introspection-only registration for a task that takes
    /// arguments and therefore needs an explicit spawn call.
    pub const fn inspectable(descriptor: &'static TaskDescriptor) -> Self {
        Self {
            descriptor,
            start: None,
        }
    }
}

/// Link-time aggregation of tasks declared with `#[task(..., linkme)]`.
///
/// The section is opt-in per task. Boards that do not use the linkme
/// attribute still link this empty slice and can use the explicit
/// fallback exclusively.
#[linkme::distributed_slice]
pub static LINKED_TASKS: [TaskRegistration] = [..];

/// Re-export so generated code can name the attribute without adding a
/// direct dependency on `linkme`.
pub use linkme::distributed_slice;

/// Fixed-capacity list of static task registrations.
#[derive(Debug)]
pub struct TaskTable<const N: usize> {
    entries: [Option<&'static TaskRegistration>; N],
    len: usize,
}

impl<const N: usize> TaskTable<N> {
    /// Create an empty task table.
    pub const fn new() -> Self {
        Self {
            entries: [None; N],
            len: 0,
        }
    }

    /// Number of registered tasks.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the table contains no tasks.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Remaining registration capacity.
    pub const fn remaining(&self) -> usize {
        N.saturating_sub(self.len)
    }

    /// Add one explicit registration.
    ///
    /// Returns `false` when the table is full, the descriptor is invalid,
    /// or a task with the same name is already present.
    pub fn register(&mut self, entry: &'static TaskRegistration) -> bool {
        if !entry.descriptor.is_valid() || self.len >= N {
            return false;
        }
        if self.contains_name(entry.descriptor.name) {
            return false;
        }
        self.entries[self.len] = Some(entry);
        self.len += 1;
        true
    }

    /// Copy every link-time registration into this table.
    ///
    /// Returns the number of entries added. Duplicate names and capacity
    /// overflows are ignored so a caller can compare `added` with
    /// [`LINKED_TASKS`] length to detect a truncated table.
    pub fn register_linked(&mut self) -> usize {
        let mut added = 0;
        for entry in LINKED_TASKS.iter() {
            if self.register(entry) {
                added += 1;
            }
        }
        added
    }

    /// Whether a task with `name` is registered.
    pub fn contains_name(&self, name: &str) -> bool {
        self.iter().any(|entry| entry.descriptor.name == name)
    }

    /// Iterate over registered entries in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &'static TaskRegistration> + '_ {
        self.entries[..self.len].iter().filter_map(|entry| *entry)
    }

    /// Launch every parameterless task.
    ///
    /// Returns `(launched, skipped)`, where `skipped` counts entries that
    /// need explicit arguments. Launchers are ordinary function pointers,
    /// so this method does not depend on the executor type and can be
    /// called from a `no_std` boot path.
    pub fn start_all(&self) -> (usize, usize) {
        let mut launched = 0;
        let mut skipped = 0;
        for entry in self.iter() {
            match entry.start {
                Some(start) => {
                    start();
                    launched += 1;
                }
                None => skipped += 1,
            }
        }
        (launched, skipped)
    }
}

impl<const N: usize> Default for TaskTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static A: TaskDescriptor = TaskDescriptor {
        name: "a",
        priority: 1,
        stack: 1024,
        pool: None,
        caps: &[],
        cpu_quota: 1000,
        mem_budget: 128,
        message_quota: 8,
    };
    static B: TaskDescriptor = TaskDescriptor {
        name: "b",
        priority: 32,
        stack: 1024,
        pool: None,
        caps: &[],
        cpu_quota: 1000,
        mem_budget: 128,
        message_quota: 8,
    };
    static REG_A: TaskRegistration = TaskRegistration::inspectable(&A);
    static REG_B: TaskRegistration = TaskRegistration::inspectable(&B);

    #[test]
    fn explicit_registration_is_bounded_and_deduplicated() {
        let mut table = TaskTable::<1>::new();
        assert!(table.register(&REG_A));
        assert!(!table.register(&REG_A));
        assert!(!table.register(&REG_B));
        assert_eq!(table.len(), 1);
        assert_eq!(table.remaining(), 0);
        assert!(table.contains_name("a"));
        assert!(!table.contains_name("b"));
    }

    #[test]
    fn invalid_priority_is_rejected() {
        let mut table = TaskTable::<2>::new();
        assert!(!table.register(&REG_B));
        assert!(table.is_empty());
    }
}
