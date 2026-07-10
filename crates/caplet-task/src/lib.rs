//! CapletRT Task Abstraction
//!
//! This crate provides the task abstraction layer for CapletRT.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

extern crate alloc;

pub mod task_id;
pub mod task;
pub mod task_config;
pub mod task_builder;
pub mod task_state;
pub mod task_context;
pub mod task_ref;
pub mod caplets;

pub use task_id::TaskId;
pub use task::Task;
pub use task_config::TaskConfig;
pub use task_builder::TaskBuilder;
pub use task_state::{TaskState, TaskPriority};
pub use task_context::TaskContext;
pub use task_ref::TaskRef;
pub use caplets::Caplets;

// Re-export CapSet from caplet-cap for convenience
pub use ::caplet_cap::{CapSet, CapKey};
