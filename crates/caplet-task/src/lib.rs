//! CapletRT Task Abstraction
//!
//! This crate provides the task abstraction layer for CapletRT.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

extern crate alloc;

pub mod caplets;
pub mod task;
pub mod task_builder;
pub mod task_config;
pub mod task_context;
pub mod task_id;
pub mod task_ref;
pub mod task_state;
pub mod task_table;

pub use caplets::Caplets;
pub use task::Task;
pub use task_builder::TaskBuilder;
pub use task_config::TaskConfig;
pub use task_context::TaskContext;
pub use task_id::TaskId;
pub use task_ref::TaskRef;
pub use task_state::{TaskPriority, TaskState};
pub use task_table::{TaskDescriptor, TaskRegistration, TaskTable};

// Re-export CapSet from caplet-cap for convenience
pub use ::caplet_cap::{CapKey, CapSet};
