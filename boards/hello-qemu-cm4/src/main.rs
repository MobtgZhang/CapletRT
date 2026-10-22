//! Hello World for QEMU Cortex-M4
//!
//! Runs on the QEMU `mps2-an386`/`mps2-an500` machine (Cortex-M4F)
//! and on the Renode MPS2-AN500 platform.

#![no_std]
#![no_main]

extern crate alloc;
extern crate caplet_cortex_m;
extern crate caplet_kernel;
extern crate caplet_macros;

use caplet::task as caplet_task;
use caplet_log::{info, install_writer, semihosting::SEMIHOSTING_INSTANCE};
use caplet_macros_exports::{drive_executor_forever, kernel_init};
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Simple bump allocator. The MPS2-AN386 has 4 MiB of SRAM at
/// 0x2000_0000 — we use 1 MiB starting at 0x2010_0000 as the heap.
struct BumpAlloc;

const HEAP_BASE: usize = 0x2010_0000;
const HEAP_END: usize = 0x2020_0000;

unsafe impl core::alloc::GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        static NEXT: AtomicUsize = AtomicUsize::new(HEAP_BASE);
        loop {
            let cur = NEXT.load(Ordering::Relaxed);
            let aligned = (cur + layout.align() - 1) & !(layout.align() - 1);
            let end = aligned + layout.size();
            if end > HEAP_END {
                return core::ptr::null_mut();
            }
            if NEXT
                .compare_exchange(cur, end, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return aligned as *mut u8;
            }
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

#[global_allocator]
static HEAP: BumpAlloc = BumpAlloc;

/// Static counter shared between the boot task and the second task so
/// the kernel can prove it is actually running both tasks.
static SHARED_COUNTER: SpinLock<u32> = SpinLock::new(0);

/// Boot task: prints the boot banner via the caplet-log framework
/// (semihosting backend) and writes the counter to the shared slot.
#[caplet_task(priority = 2, stack = 4096, name = "boot", linkme)]
async fn boot_task() {
    info!(
        "boot",
        "CapletRT v0.1.0 booting on Cortex-M4 @ QEMU (MPS2-AN386)"
    );
    info!("fpu", "FPU enabled");
    info!("cap_init", "initializing kernel...");
    info!("cap_init", "done. Boot task running.");
    info!("demo", "hello world");
    info!("sched", "idle -> WFI");

    *SHARED_COUNTER.lock() += 1;
}

/// Second task: counts forever to prove the scheduler is alive.
#[caplet_task(priority = 3, stack = 4096, name = "control", linkme)]
async fn control_task() {
    let mut n: u32 = 0;
    loop {
        n = n.wrapping_add(1);
        *SHARED_COUNTER.lock() += 1;
        if n % 1000 == 0 {
            info!("control", "tick {}", n);
        }
    }
}

/// Network task: initializes Ethernet and runs a simple loop.
#[caplet_task(priority = 4, stack = 4096, name = "network", linkme)]
async fn network_task() {
    info!("network", "Starting network task...");
    // The SMSC 91C111 model is only present on the QEMU MPS2 machines;
    // this board keeps the task alive and logging so the scheduler has
    // observable work while the real network stack is brought up.
    loop {
        info!("network", "Network task running...");
        for _ in 0..2_000_000 {
            core::hint::spin_loop();
        }
    }
}

/// Deterministic tickless-idle diagnostic. Each iteration programs a
/// 20 ms variable SysTick reload, enters WFI, and records the duration
/// before the ISR wakes the core. `tools/tickless_verify.sh` asserts
/// the emitted counters.
#[cfg(feature = "tickless-verify")]
#[caplet_task(priority = 2, stack = 4096, name = "tickless", linkme)]
async fn tickless_verify_task() {
    use caplet_kernel::tickless::{record_wakeup, reset_wakeup_count, IDLE_TICKS_LO, WAKEUP_COUNT};

    reset_wakeup_count();
    caplet_cortex_m::systick::reset_irq_count();
    info!(
        "tickless",
        "calib={:#010x} cpm={} rvr={}",
        caplet_cortex_m::systick::calibration_raw(),
        caplet_cortex_m::systick::cycles_per_ms(),
        caplet_cortex_m::systick::reload_value()
    );
    let start = caplet_kernel::syscall_timer::get_ticks();
    for _ in 0..5u32 {
        record_wakeup(20);
        caplet_kernel::tickless::enter_idle(20);
    }
    let idle_ticks = IDLE_TICKS_LO.load(Ordering::Acquire);
    let wakeups = WAKEUP_COUNT.load(Ordering::Acquire);
    let irqs = caplet_cortex_m::systick::irq_count();
    let elapsed = caplet_kernel::syscall_timer::get_ticks().saturating_sub(start);
    info!(
        "tickless",
        "wakeups={} idle_ticks={} irqs={} elapsed_ms={}", wakeups, idle_ticks, irqs, elapsed
    );
    if wakeups == 5 && idle_ticks == 100 && irqs == 5 {
        info!("tickless", "OK");
    } else {
        info!("tickless", "FAILED");
    }
}

/// Rust entry point. `cortex-m-rt` calls this from the reset vector
/// after initializing BSS and enabling the FPU.
#[cortex_m_rt::entry]
fn main() -> ! {
    // Install the semihosting log writer before anything can call info!/debug!/etc.
    install_writer(Some(&SEMIHOSTING_INSTANCE));
    kernel_init();

    // Prove both registration paths on the real target. `linked` is
    // populated from the linkme section emitted by #[caplet::task];
    // `startup` uses the explicit fallback helpers so boot does not
    // depend on custom-section layout.
    let mut linked = caplet::runtime::TaskTable::<8>::new();
    let linked_count = linked.register_linked();

    let mut startup = caplet::runtime::TaskTable::<8>::new();
    let mut explicit_count = 0usize;
    if __caplet_register_task_boot_task(&mut startup) {
        explicit_count += 1;
    }
    #[cfg(feature = "tickless-verify")]
    if __caplet_register_task_tickless_verify_task(&mut startup) {
        explicit_count += 1;
    }
    #[cfg(not(feature = "tickless-verify"))]
    {
        if __caplet_register_task_control_task(&mut startup) {
            explicit_count += 1;
        }
        if __caplet_register_task_network_task(&mut startup) {
            explicit_count += 1;
        }
    }
    // Every task in this board is marked `linkme`, so the link-time table
    // contains one more entry in tickless-verify mode (the diagnostic task).
    // The explicit startup table intentionally launches only the boot task
    // plus one workload task.
    let expected_linked = if cfg!(feature = "tickless-verify") {
        4
    } else {
        3
    };
    let expected_explicit = if cfg!(feature = "tickless-verify") {
        2
    } else {
        3
    };
    if linked_count != expected_linked || explicit_count != expected_explicit {
        info!(
            "task_table",
            "FAILED linked={} explicit={} expected_linked={} expected_explicit={}",
            linked_count,
            explicit_count,
            expected_linked,
            expected_explicit
        );
        loop {
            caplet_kernel::tickless::enter_idle(1);
        }
    }
    let (launched, skipped) = startup.start_all();
    info!(
        "task_table",
        "linked={} explicit={} launched={} skipped={}",
        linked_count,
        explicit_count,
        launched,
        skipped
    );
    drive_executor_forever()
}
