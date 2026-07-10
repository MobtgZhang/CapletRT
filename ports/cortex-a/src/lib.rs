//! CapletRT Cortex-A Port (M3)
//!
//! Architecture port for the ARM Cortex-A family. The target of
//! choice for M3 development and CI is the QEMU `vexpress-a9` board
//! (dual-core Cortex-A9, GICv2, no MMU walk-through required to get
//! `hello world` running).
//!
//! TCB: this crate, together with the kernel core, is part of the
//! Trusted Computing Base. Anything outside these is treated as
//! untrusted by the kernel.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

pub mod boot;
pub mod context;
pub mod gic;
pub mod kernel_bridge;
pub mod mmu;
pub mod panic;
pub mod pl011_uart;
pub mod smc;
pub mod tick;

pub use boot::cortex_a_boot;
pub use context::TaskContext;
pub use mmu::Mmu;

use caplet_kernel as _kernel;

/// AArch32 (32-bit ARM) PL011 UART helpers exposed for boards.
pub use pl011_uart::Pl011Uart;

/// Re-export of kernel PagePerms for convenience.
pub use _kernel::mmu::PagePerms as KPagePerms;
