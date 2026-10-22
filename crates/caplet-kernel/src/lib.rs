//! CapletRT Kernel Core
//!
//! This crate implements the core kernel functionality for CapletRT.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]
// MSRV 1.75 predates `is_multiple_of`; the modulo form is intentional.
#![allow(clippy::manual_is_multiple_of)]

extern crate alloc;

// Required for no_std with alloc
extern crate linked_list_allocator;

pub mod boot;
pub mod bump_allocator;
pub mod kernel;
pub mod kernel_config;
pub mod kernel_init;
pub mod main_task;
pub mod memory;
#[cfg(feature = "module-loader")]
pub mod module;
pub mod priority;
pub mod quota;
pub mod ready_queue;
pub mod resource_table;
pub mod scheduler;
pub mod security_grants;
pub mod slot_allocator;
pub mod syscall;
pub mod syscall_cap;
pub mod syscall_crypto; // M4: crypto syscall handler
pub mod syscall_dispatch;
pub mod syscall_flash; // M4: flash syscall handler
pub mod syscall_ipc;
pub mod syscall_m3;
pub mod syscall_task;
pub mod tickless;
// M3 additions
pub mod arch;
pub mod cpu;
pub mod ipc;
pub mod ipc_namespace;
pub mod mmu;
pub mod process;
pub mod smp;
pub mod smp_scheduler;

pub mod syscall_memory;
pub mod syscall_timer;

pub use arch::{Arch, MmuConfig};
pub use boot::boot_task_main;
pub use cpu::{cpu_count, current_cpu, set_cpu_count, CpuId, IpiReason, MAX_CPUS};
pub use ipc_namespace::{EndpointHandle, EndpointName, IpcNamespace, NamespaceId};
pub use kernel::{yield_current_task, Kernel};
pub use kernel_config::KernelConfig;
pub use kernel_init::{cap_init, cap_init_with_config};
pub use memory::MemoryLayout;
pub use priority::Priority;
pub use process::{Process, ProcessId, ProcessState};
pub use scheduler::Scheduler;
pub use smp_scheduler::SmpScheduler;
pub use syscall::Syscall;
