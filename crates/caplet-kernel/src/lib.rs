//! CapletRT Kernel Core
//!
//! This crate implements the core kernel functionality for CapletRT.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

extern crate alloc;

// Required for no_std with alloc
extern crate linked_list_allocator;

pub mod kernel;
pub mod kernel_init;
pub mod kernel_config;
pub mod scheduler;
pub mod priority;
pub mod ready_queue;
pub mod syscall;
pub mod syscall_dispatch;
pub mod syscall_cap;
pub mod syscall_task;
pub mod syscall_ipc;
pub mod syscall_m3;
pub mod syscall_crypto; // M4: crypto syscall handler
pub mod syscall_flash;  // M4: flash syscall handler
pub mod memory;
pub mod bump_allocator;
pub mod slot_allocator;
pub mod boot;
pub mod main_task;
pub mod resource_table;
pub mod tickless;
// M3 additions
pub mod cpu;
pub mod arch;
pub mod smp_scheduler;
pub mod smp;
pub mod process;
pub mod ipc_namespace;
pub mod mmu;
pub mod ipc;

pub mod syscall_timer;
pub mod syscall_memory;

pub use kernel::Kernel;
pub use kernel_init::cap_init;
pub use kernel_config::KernelConfig;
pub use scheduler::Scheduler;
pub use priority::Priority;
pub use syscall::Syscall;
pub use memory::MemoryLayout;
pub use boot::boot_task_main;
pub use cpu::{CpuId, IpiReason, MAX_CPUS, current_cpu, cpu_count, set_cpu_count};
pub use arch::{Arch, MmuConfig};
pub use smp_scheduler::SmpScheduler;
pub use process::{Process, ProcessState, ProcessId};
pub use ipc_namespace::{IpcNamespace, NamespaceId, EndpointName, EndpointHandle};
