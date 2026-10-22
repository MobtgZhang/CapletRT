//! CapletRT AArch64 (ARM64) architecture port.
//!
//! Targets the QEMU `virt` machine and the Renode Cortex-A53/GICv3
//! platform. Both start the image at EL1 with the MMU disabled.
//!
//! Memory map (128 MiB configuration):
//!   0x0900_0000  PL011 UART0
//!   0x0800_0000  GICv3 distributor
//!   0x4000_0000  DRAM / kernel image
//!   0x4800_0000  end of DRAM (initial stack)

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

pub mod arch;
pub mod boot;
pub mod context;
pub mod gic;
pub mod kernel_bridge;
pub mod mmu;
pub mod panic;
pub mod tick;
pub mod uart;

pub use arch::AArch64Arch;
pub use context::TaskContext;
pub use gic::Version as GicVersion;
pub use kernel_bridge::kernel_main;
pub use tick::TickTimer;
pub use uart::Uart;

#[cfg(target_arch = "aarch64")]
pub use context::{__caplet_start_first, __caplet_switch};
