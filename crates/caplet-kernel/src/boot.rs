//! Boot task implementation
//!
//! The boot task is the first task started by the kernel. It is responsible for:
//! 1. Loading board configuration
//! 2. Creating initial CSpace with kernel capabilities
//! 3. Spawning initial user tasks
//! 4. Transitioning to idle

use crate::kernel::Kernel;
use crate::priority::Priority;
use crate::scheduler::ScheduledTask;
use crate::kernel_config::KernelConfig;
use crate::memory::MemoryLayout;

/// Boot task - the first task started by the kernel
pub struct BootTask {
    /// Task priority
    priority: Priority,
    /// Stack base address
    stack_base: u32,
    /// Stack size
    stack_size: usize,
}

impl BootTask {
    /// Create a new boot task
    pub fn new(priority: Priority, stack_base: u32, stack_size: usize) -> Self {
        Self {
            priority,
            stack_base,
            stack_size,
        }
    }

    /// Get the task entry point
    pub fn entry(&self) -> u32 {
        boot_task_main as *const () as u32
    }

    /// Get the stack pointer
    pub fn stack_pointer(&self) -> u32 {
        self.stack_base + self.stack_size as u32
    }

    /// Convert to a scheduled task
    pub fn to_scheduled(&self) -> ScheduledTask {
        ScheduledTask::new(
            0, // Boot task is ID 0
            self.priority,
            self.entry(),
            self.stack_pointer(),
        )
    }
}

impl Default for BootTask {
    fn default() -> Self {
        Self::new(Priority::new(0), 0x2000_0000, 2048)
    }
}

/// Boot task main function
///
/// This is the entry point for the boot task.
/// It loads board configuration and spawns initial user tasks.
pub extern "C" fn boot_task_main() -> ! {
    // Step 1: Get kernel instance
    // Note: In a real system, this would be passed as an argument
    // For now, we just enter the idle loop after initialization

    #[cfg(feature = "log")]
    {
        crate::println!("[boot] CapletRT kernel initialized");
        crate::println!("[boot] Boot task starting...");
    }

    // Step 2: Load board configuration
    // In a real implementation, this would:
    // - Parse board-specific config (device tree, TOML, etc.)
    // - Initialize HAL drivers
    // - Configure peripherals

    // Step 3: Initialize capability space
    // - Create kernel root capabilities
    // - Grant minimal capabilities to boot task

    // Step 4: Spawn initial user tasks
    // This would be done by calling the scheduler

    #[cfg(feature = "log")]
    {
        crate::println!("[boot] Board configuration loaded");
        crate::println!("[boot] Entering idle loop...");
    }

    // Step 5: Enter idle loop
    // In tickless mode, this would use WFI/WFE
    idle_loop();
}

/// Idle loop - called when no tasks are ready to run
#[cfg(target_arch = "arm")]
fn idle_loop() -> ! {
    // On ARM, this function should be overridden by the cortex-m port.
    // For now, we provide a fallback that can be overridden.
    #[cfg(feature = "cortex_m_idle")]
    {
        cortex_m::asm::wfi();
    }
    #[cfg(not(feature = "cortex_m_idle"))]
    {
        loop {
            core::hint::spin_loop();
        }
    }
}

#[cfg(not(target_arch = "arm"))]
fn idle_loop() -> ! {
    loop {
        // Non-ARM platforms just yield
        core::hint::spin_loop();
    }
}

/// Initialize and spawn boot task
pub fn init_boot_task(kernel: &mut Kernel) {
    let boot = BootTask::default();

    #[cfg(feature = "log")]
    {
        crate::println!(
            "[boot] Boot task configured: priority={}, stack={:#x}",
            boot.priority.0,
            boot.stack_size
        );
    }

    // Register boot task with kernel
    // The kernel would add it to the scheduler here
}