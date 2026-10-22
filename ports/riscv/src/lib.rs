//! CapletRT RISC-V Port
//!
//! Architecture port for RISC-V processors (RV32IMAC / RV64GC).
//! Targets QEMU `virt` and Renode `riscv_virt` as the primary
//! development platforms.
//!
//! Memory layout (QEMU virt):
//!   Main RAM:    0x8000_0000..0x8800_0000 (128 MiB)
//!   CLINT:       0x0200_0000..0x0200_FFFF
//!   PLIC:        0x0c00_0000..0x1000_0000
//!   UART0:       0x1000_0000..0x1000_1000
//!
//! Boot flow:
//!   reset (M-mode) → PMP + delegation + mret
//!         → S-mode s_mode_entry → riscv_boot() → BSS + UART
//!         → early_init → board `rust_main`
//!
//! Both QEMU and Renode enter the ELF in M-mode. `boot.rs` performs the
//! M→S transition; the kernel itself executes in S-mode.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

pub mod arch;
pub mod boot;
pub mod clint;
pub mod context;
pub mod kernel_bridge;
pub mod log;
pub mod mmu;
pub mod panic;
pub mod plic;
pub mod tick;
pub mod uart;

pub use arch::RiscVArch;
pub use boot::riscv_boot;
pub use clint::Clint;
pub use context::{build_initial_stack, pend_software_irq, TaskContext};
pub use plic::Plic;
pub use tick::TickTimer;
pub use uart::Uart;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub use context::{__caplet_start_first, __caplet_switch};
pub use log::{UartLogWriter, UART_LOG};
