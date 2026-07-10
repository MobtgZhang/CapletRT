//! CapletRT Stackless Async Executor
//!
//! This crate provides a minimal stackless async executor for CapletRT.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

extern crate alloc;

pub mod executor;
pub mod waker;
pub mod wake_queue;
pub mod wake_source;
pub mod async_task;
pub mod spawner;
pub mod block_on;

pub use executor::Executor;
pub use wake_queue::WakeQueue;
pub use wake_source::WakeSource;
pub use async_task::AsyncTask;
pub use spawner::{Spawner, TaskHandle};
pub use block_on::block_on;