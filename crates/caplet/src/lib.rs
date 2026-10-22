//! CapletRT umbrella crate.
//!
//! This is the dependency board crates normally use. It re-exports the
//! procedural macros (`main`, `task`), the executor/task types used by
//! macro-generated code, and the `runtime` module that contains the
//! stable entry points referenced by `caplet-macros`.

#![no_std]
#![deny(missing_docs)]

/// Runtime helpers used by generated code.
///
/// The name is intentionally stable: the procedural macros emit paths
/// such as `::caplet::runtime::spawn_main_with`.
pub mod runtime {
    pub use caplet_macros_exports::*;
}

pub use caplet_async::{Executor, Spawner, TaskHandle, WakeQueue};
pub use caplet_cap::{CSpace, Cap, CapError, CapKey, CapResult, CapSet, Handle, Permissions};
pub use caplet_macros::{main, task};
pub use caplet_task::{Caplets, Task, TaskBuilder, TaskConfig, TaskId, TaskPriority, TaskState};

pub use runtime::{
    drive_executor, drive_executor_forever, drive_executor_once, kernel_init, kernel_start,
    spawn_main, spawn_main_with, spawn_with_priority, TaskDescriptor,
};

/// The kernel crate, exposed for boards that need platform-specific
/// initialisation hooks without adding a second dependency.
pub use caplet_kernel as kernel;
