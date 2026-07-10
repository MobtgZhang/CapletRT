//! Architecture abstraction layer.
//!
//! CapletRT supports multiple architectures (Cortex-M, Cortex-A, RISC-V
//! in the future). The kernel core is arch-agnostic and calls into the
//! port layer through this trait for the bits that cannot be
//! expressed in portable Rust:
//!
//! - early init (interrupt mask, BSS zero, vector relocation)
//! - MMU/MPU configuration (no-op on architectures without one)
//! - context switch (the register shuffle that hands the CPU from one
//!   task to another)
//! - inter-processor interrupts (used for SMP reschedule and TLB
//!   shootdown)
//! - memory ordering (relaxed on single-core, full barrier on SMP)
//! - monotonic tick counter
//!
//! Each architecture provides a type implementing this trait. The
//! kernel selects one at link time via a `cfg`-gated type alias.

use crate::cpu::{CpuId, IpiReason};

/// MMU configuration handed to the architecture at boot time. On
/// architectures without an MMU the fields are unused.
#[derive(Debug, Clone, Copy)]
pub struct MmuConfig {
    /// Virtual base where the kernel is linked.
    pub kernel_vaddr: u32,
    /// Physical base of the kernel image.
    pub kernel_paddr: u32,
    /// Physical base of the pool used for dynamically loaded modules.
    pub modules_pool_paddr: u32,
    /// Size of the module pool in bytes.
    pub modules_pool_size: usize,
    /// Physical base of the IPC shared memory region.
    pub ipc_shm_paddr: u32,
    /// Size of the IPC shared memory region.
    pub ipc_shm_size: usize,
}

impl MmuConfig {
    /// Configuration suitable for the QEMU vexpress-a9 board.
    pub const fn vexpress_a9() -> Self {
        Self {
            kernel_vaddr: 0x4000_0000,
            kernel_paddr: 0x4000_0000,
            modules_pool_paddr: 0x6000_0000,
            modules_pool_size: 16 * 1024 * 1024,
            ipc_shm_paddr: 0x7000_0000,
            ipc_shm_size: 4 * 1024 * 1024,
        }
    }

    /// No-op config for architectures that have no MMU.
    pub const fn no_mmu() -> Self {
        Self {
            kernel_vaddr: 0,
            kernel_paddr: 0,
            modules_pool_paddr: 0,
            modules_pool_size: 0,
            ipc_shm_paddr: 0,
            ipc_shm_size: 0,
        }
    }
}

/// Architecture abstraction implemented by every port.
pub trait Arch {
    /// Human-readable architecture name.
    fn name() -> &'static str;
    /// Maximum number of CPUs the architecture supports.
    fn max_cpus() -> u32;
    /// Run the architecture's early boot sequence. Disables
    /// interrupts, zeroes BSS, sets up the vector table.
    fn early_init();
    /// Enable the MMU with the given configuration. No-op on
    /// architectures that have no MMU.
    fn enable_mmu(_cfg: &MmuConfig) {}
    /// Switch context from `prev` to `next`. Both pointers are
    /// pointers to the architecture's saved context layout.
    ///
    /// # Safety
    /// The pointers must each be valid for a register-frame save done
    /// by a previous `__caplet_switch` call (or the initial context
    /// built by the port's `build_initial_context`).
    unsafe fn context_switch(prev: *mut u32, next: *const u32);
    /// Run the first task. The `next` context was built by
    /// `build_initial_context` and is consumed (not saved back).
    ///
    /// # Safety
    /// `next` must point to a freshly built initial context whose
    /// `pc` is a valid entry point. The function does not return.
    unsafe fn start_first_task(next: *const u32) -> !;
    /// Send an inter-processor interrupt.
    fn ipi_send(_target: CpuId, _reason: IpiReason, _arg: u32) {}
    /// Acknowledge the last IPI. Called from the IPI handler.
    fn ipi_ack(_cpu: CpuId) {}
    /// Full memory barrier. On single-core architectures a compiler
    /// fence is sufficient; SMP architectures need a hardware
    /// barrier.
    fn fence() {
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }
    /// Read the architecture's monotonic tick counter.
    fn now_ticks() -> u64;
    /// Pend a context-switch request. On Cortex-M this triggers
    /// PendSV; on Cortex-A it sends an SGI to the current CPU.
    fn request_reschedule();
    /// Halt the calling CPU until the next interrupt.
    fn wait_for_interrupt();
}
