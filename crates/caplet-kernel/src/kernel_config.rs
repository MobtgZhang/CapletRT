//! Kernel configuration

use crate::memory::MemoryLayout;
use crate::priority::Priority;

/// Kernel configuration parameters
pub struct KernelConfig {
    /// Memory layout for the system
    pub memory: MemoryLayout,
    /// Number of tasks supported
    pub max_tasks: usize,
    /// Number of interrupts supported
    pub max_interrupts: usize,
    /// Number of IPC channels
    pub max_channels: usize,
    /// Default task priority
    pub default_priority: Priority,
    /// Number of priority levels
    pub priority_levels: u8,
    /// Enable tickless idle
    pub tickless_idle: bool,
    /// SysTick frequency in Hz
    pub systick_hz: u32,
    /// Enable audit logging
    pub audit_log: bool,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            memory: MemoryLayout::default(),
            max_tasks: 16,
            max_interrupts: 32,
            max_channels: 8,
            default_priority: Priority::default(),
            priority_levels: 32,
            tickless_idle: true,
            systick_hz: 1000,
            audit_log: false,
        }
    }
}

impl KernelConfig {
    /// Create minimal configuration for embedded systems
    pub fn minimal() -> Self {
        Self {
            memory: MemoryLayout::minimal(),
            max_tasks: 4,
            max_interrupts: 16,
            max_channels: 2,
            default_priority: Priority::default(),
            priority_levels: 8,
            tickless_idle: false,
            systick_hz: 100,
            audit_log: false,
        }
    }

    /// Create QEMU configuration
    pub fn qemu() -> Self {
        #[cfg(target_arch = "riscv64")]
        let memory = MemoryLayout::riscv_virt();
        #[cfg(target_arch = "riscv32")]
        let memory = MemoryLayout::riscv_virt();
        #[cfg(target_arch = "aarch64")]
        let memory = MemoryLayout::aarch64_virt();
        #[cfg(not(any(
            target_arch = "riscv64",
            target_arch = "riscv32",
            target_arch = "aarch64"
        )))]
        let memory = MemoryLayout::qemu();

        Self {
            memory,
            max_tasks: 8,
            max_interrupts: 32,
            max_channels: 4,
            default_priority: Priority::default(),
            priority_levels: 16,
            tickless_idle: true,
            systick_hz: 1000,
            audit_log: false,
        }
    }

    /// Configuration for the QEMU RISC-V `virt` machine.
    pub fn riscv_virt() -> Self {
        let mut cfg = Self::qemu();
        cfg.memory = MemoryLayout::riscv_virt();
        cfg
    }

    /// Configuration for the QEMU AArch64 `virt` machine.
    pub fn aarch64_virt() -> Self {
        let mut cfg = Self::qemu();
        cfg.memory = MemoryLayout::aarch64_virt();
        cfg
    }

    /// Configuration for the QEMU `vexpress-a9` / Renode Cortex-A9 and
    /// Allwinner A20 images.
    pub fn cortex_a9() -> Self {
        let mut cfg = Self::qemu();
        cfg.memory = MemoryLayout::vexpress_a9();
        cfg
    }
}
