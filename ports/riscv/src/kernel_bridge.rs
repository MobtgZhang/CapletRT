//! Bridge between the RISC-V port and the kernel core.
//!
//! A board provides `rust_main`, so the port must not define a second
//! symbol with that name. This module exposes a helper that boards may
//! call after their own early output if they want to start the kernel.
//!
//! The helper installs the architecture's native identity map, creates
//! two cooperative tasks and runs the capability kernel scheduler. It
//! deliberately uses only the public kernel API so the same code works
//! on QEMU and Renode, with RV32 and RV64.

extern crate alloc;

use crate::arch::RiscVArch;
use crate::context::build_initial_stack;
use crate::uart::Uart;
use caplet_kernel::arch::Arch;
use caplet_kernel::scheduler::ScheduledTask;
use caplet_kernel::{cap_init_with_config, yield_current_task, KernelConfig, Priority};

/// Stack size for the two demonstration tasks.
const DEMO_STACK_SIZE: usize = 16 * 1024;
/// Number of times each task yields before parking.
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
    // Let the other task observe completion before parking.
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
            Uart::put_line("CapletRT RISC-V kernel scheduler OK");
        }
        yield_current_task();
    }
    Uart::put_line("kernel: task B done");
    park()
}

fn park() -> ! {
    loop {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            // SAFETY: WFI is safe in a parking loop.
            unsafe { core::arch::asm!("wfi", options(nostack, preserves_flags)) };
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        core::hint::spin_loop();
    }
}

/// Adapter matching the kernel's `ContextSwitchFn` ABI.  The `Arch`
/// trait uses a const `next` pointer; the assembly routine itself only
/// reads through it.
unsafe extern "C" fn kernel_switch(prev: *mut usize, next: *mut usize) {
    // SAFETY: forwarded from `Kernel::start_with_context_switch`.
    unsafe { crate::context::__caplet_switch(prev, next as *const usize) }
}

fn make_task(id: u32, priority: u8, entry: usize, arg: usize, stack_top: usize) -> ScheduledTask {
    let sp = build_initial_stack(stack_top as *mut usize, entry, arg) as usize;
    let mut task = ScheduledTask::new(id, Priority::new(priority), entry, sp)
        .with_stack_size(DEMO_STACK_SIZE as u32);
    task.context = sp;
    task
}

/// Start the capability kernel. Boards call this from their own
/// `rust_main` after platform-specific bring-up.
pub extern "C" fn kernel_main() -> ! {
    Uart::init();
    Uart::put_line("CapletRT booting on RISC-V");

    RiscVArch::early_init();

    // Identity-map devices and the full 128 MiB RAM window. The board
    // already prints `CapletRT RISC-V boot OK` before calling us, so the
    // marker remains the stable simulator contract even if a platform
    // lacks Sv39/Sv32 support.
    let mmu = caplet_kernel::arch::MmuConfig {
        kernel_vaddr: crate::boot::DRAM_BASE as u32,
        kernel_paddr: crate::boot::DRAM_BASE as u32,
        modules_pool_paddr: 0x8200_0000,
        modules_pool_size: 8 * 1024 * 1024,
        ipc_shm_paddr: 0x8280_0000,
        ipc_shm_size: 4 * 1024 * 1024,
    };
    RiscVArch::enable_mmu(&mmu);
    if cfg!(any(target_arch = "riscv32", target_arch = "riscv64")) {
        Uart::put_line("mmu: Sv39/Sv32 identity map enabled");
    } else {
        Uart::put_line("mmu: paging unavailable, physical identity mode");
    }

    Uart::put_line("kernel: initialising capability kernel");
    let kernel = cap_init_with_config(KernelConfig::riscv_virt());
    Uart::put_line("kernel: capability kernel ready");

    // SAFETY: mutable statics are only accessed here, during
    // single-threaded boot, to obtain stable stack addresses.
    let top_a = unsafe { core::ptr::addr_of_mut!(TASK_A_STACK.0) } as usize + DEMO_STACK_SIZE;
    let top_b = unsafe { core::ptr::addr_of_mut!(TASK_B_STACK.0) } as usize + DEMO_STACK_SIZE;
    // SAFETY: the stacks are static, aligned and only used by these two
    // tasks. The task entry points never return.
    let task_a = make_task(1, 10, demo_task_a as *const () as usize, 0xA1, top_a);
    let task_b = make_task(2, 20, demo_task_b as *const () as usize, 0xB2, top_b);
    Uart::put_line("kernel: contexts built");
    kernel.add_task(task_a);
    kernel.add_task(task_b);
    Uart::put_line("kernel: tasks enqueued");

    Uart::put_line("kernel: starting cooperative scheduler");
    kernel.start_with_context_switch(kernel_switch)
}
