//! Bridge between the AArch64 port and the capability kernel.
//!
//! The helper installs the identity map, creates two cooperative tasks
//! and runs the real kernel scheduler.  It is shared by QEMU `virt`
//! (GICv2) and Renode's Cortex-A53 platform (GICv3).

extern crate alloc;

use crate::arch::AArch64Arch;
use crate::context::build_initial_stack;
use crate::uart::Uart;
use caplet_kernel::arch::Arch;
use caplet_kernel::scheduler::ScheduledTask;
use caplet_kernel::{cap_init_with_config, yield_current_task, KernelConfig, Priority};

/// Stack size for each demonstration task.
const DEMO_STACK_SIZE: usize = 16 * 1024;
/// Number of cooperative rounds per task.
const DEMO_ROUNDS: usize = 4;

#[repr(align(16))]
struct TaskStack([u8; DEMO_STACK_SIZE]);

static mut TASK_A_STACK: TaskStack = TaskStack([0; DEMO_STACK_SIZE]);
static mut TASK_B_STACK: TaskStack = TaskStack([0; DEMO_STACK_SIZE]);

extern "C" fn demo_task_a(arg: usize) -> ! {
    for round in 0..DEMO_ROUNDS {
        Uart::put_str("kernel: task A arg=");
        Uart::put_hex(arg as u64);
        Uart::put_str(" round=");
        Uart::put_hex(round as u64);
        Uart::put_line("");
        yield_current_task();
    }
    Uart::put_line("kernel: task A done");
    // A final yield lets task B run to completion before this stack is
    // parked permanently.
    yield_current_task();
    park()
}

extern "C" fn demo_task_b(arg: usize) -> ! {
    for round in 0..DEMO_ROUNDS {
        Uart::put_str("kernel: task B arg=");
        Uart::put_hex(arg as u64);
        Uart::put_str(" round=");
        Uart::put_hex(round as u64);
        Uart::put_line("");
        if round == 0 {
            Uart::put_line("CapletRT AArch64 kernel scheduler OK");
        }
        yield_current_task();
    }
    Uart::put_line("kernel: task B done");
    park()
}

fn park() -> ! {
    loop {
        #[cfg(target_arch = "aarch64")]
        // SAFETY: WFE is a low-power hint with no memory side effects.
        unsafe {
            core::arch::asm!("wfe", options(nomem, nostack));
        }
        #[cfg(not(target_arch = "aarch64"))]
        core::hint::spin_loop();
    }
}

/// Adapter matching the kernel's `ContextSwitchFn` ABI.
unsafe extern "C" fn kernel_switch(prev: *mut usize, next: *mut usize) {
    // SAFETY: forwarded from `Kernel::start_with_context_switch` and
    // the AArch64 context assembly routine.
    unsafe { crate::context::__caplet_switch(prev, next as *const usize) }
}

fn make_task(id: u32, priority: u8, entry: usize, arg: usize, stack_top: usize) -> ScheduledTask {
    let sp = build_initial_stack(stack_top as *mut u64, entry as u64, arg as u64) as usize;
    let mut task = ScheduledTask::new(id, Priority::new(priority), entry, sp)
        .with_stack_size(DEMO_STACK_SIZE as u32);
    task.context = sp;
    task
}

/// Start the capability kernel on AArch64.
pub extern "C" fn kernel_main() -> ! {
    Uart::init();
    AArch64Arch::early_init();

    let mmu = caplet_kernel::arch::MmuConfig {
        kernel_vaddr: crate::boot::DRAM_BASE as u32,
        kernel_paddr: crate::boot::DRAM_BASE as u32,
        modules_pool_paddr: 0x4200_0000,
        modules_pool_size: 8 * 1024 * 1024,
        ipc_shm_paddr: 0x4280_0000,
        ipc_shm_size: 4 * 1024 * 1024,
    };
    AArch64Arch::enable_mmu(&mmu);
    Uart::put_line("mmu: AArch64 identity map enabled (4 KiB granule)");

    match crate::gic::version() {
        crate::gic::Version::V2 => Uart::put_line("gic: GICv2 @ 0x08000000"),
        crate::gic::Version::V3 => Uart::put_line("gic: GICv3 @ 0x08000000 / GICR 0x080a0000"),
        crate::gic::Version::Unknown(revision) => {
            Uart::put_str("gic: unknown revision ");
            Uart::put_hex(revision as u64);
            Uart::put_line("");
        }
    }

    Uart::put_line("kernel: initialising capability kernel");
    let kernel = cap_init_with_config(KernelConfig::aarch64_virt());
    Uart::put_line("kernel: capability kernel ready");

    // SAFETY: these statics are only read during single-threaded boot to
    // obtain the tops of the two dedicated task stacks.
    let top_a = unsafe { core::ptr::addr_of_mut!(TASK_A_STACK.0) } as usize + DEMO_STACK_SIZE;
    let top_b = unsafe { core::ptr::addr_of_mut!(TASK_B_STACK.0) } as usize + DEMO_STACK_SIZE;
    let task_a = make_task(1, 10, demo_task_a as *const () as usize, 0xA1, top_a);
    let task_b = make_task(2, 20, demo_task_b as *const () as usize, 0xB2, top_b);
    Uart::put_line("kernel: contexts built");
    kernel.add_task(task_a);
    kernel.add_task(task_b);
    Uart::put_line("kernel: tasks enqueued");

    Uart::put_line("kernel: starting cooperative scheduler");
    kernel.start_with_context_switch(kernel_switch)
}
