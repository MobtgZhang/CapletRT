//! CapletRT RISC-V Port
//!
//! Architecture port for RISC-V processors (RV32IMAC / RV64GC).
//! Targets QEMU `virt` board as the primary development platform.
//!
//! Memory layout (QEMU virt):
//!   Flash (ROM): 0x1000_0000..0x1008_0000 (512 KiB, simulated)
//!   Main RAM:    0x8000_0000..0x8800_0000 (128 MiB)
//!   CLINT:       0x0200_0000..0x0200_FFFF
//!   PLIC:        0x0c00_0000..0x1000_0000
//!   UART0:       0x1000_0000..0x1000_1000
//!
//! Boot flow:
//!   reset → riscv_boot() → early_init (BSS, PMP, mtvec)
//!         → enable_mmu (optional) → rust_main() → caplet_kernel::cap_init()
//!
//! Context switching uses S-mode (Supervisor mode). On QEMU virt the
//! boot ROM loads us directly into S-mode with MMU disabled.

#![deny(missing_docs)]
#![allow(unsafe_code)]
#![no_std]

pub mod arch;
pub mod boot;
pub mod clint;
pub mod context;
pub mod kernel_bridge;
pub mod mmu;
pub mod panic;
pub mod plic;
pub mod tick;
pub mod uart;

pub use arch::RiscVArch;
pub use boot::riscv_boot;
pub use clint::Clint;
pub use context::{TaskContext, build_initial_stack, pend_software_irq};
pub use plic::Plic;
pub use tick::TickTimer;
pub use uart::Uart;

#[cfg(target_arch = "riscv")]
pub use context::{__caplet_switch, __caplet_start_first};
