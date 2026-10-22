//! CapletRT Cortex-M Port
//!
//! Architecture port for ARM Cortex-M processors.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

pub mod boot;
pub mod context;
pub mod interrupt;
pub mod kernel_bridge;
pub mod panic;
pub mod qemu_uart;
pub mod systick;
pub mod tickless;

pub use boot::cortex_m_boot;
#[cfg(target_arch = "arm")]
pub use context::{__caplet_start_first, __caplet_switch};
pub use context::{
    build_initial_stack, pend_pendsv, read_psp, set_pendsv_handler, set_psp, set_svc_handler,
    ExceptionHook, PspSlot, TaskContext,
};
pub use interrupt::{ExceptionFrame, InterruptFrame};
