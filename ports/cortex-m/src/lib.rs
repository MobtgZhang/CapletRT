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
pub use interrupt::{InterruptFrame, ExceptionFrame};
pub use context::{TaskContext, build_initial_stack, pend_pendsv, set_psp, read_psp, PspSlot};
#[cfg(target_arch = "arm")]
pub use context::{__caplet_switch, __caplet_start_first};

