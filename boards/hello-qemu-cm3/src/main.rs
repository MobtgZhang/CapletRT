//! Hello World for QEMU Cortex-M3
//!
//! Minimal demo that runs on QEMU with mps2-an385 machine.
//!
//! The Cortex-M3 lacks full atomic CAS support, so we avoid the
//! stackless async executor and drive the kernel directly. The boot
//! banner is printed and the kernel enters its idle loop.

#![no_std]
#![no_main]
#![deny(warnings)]

extern crate alloc;

extern crate caplet_cortex_m;
extern crate caplet_kernel;
extern crate caplet_macros;
extern crate caplet_macros_exports;

use caplet::task as caplet_task;
use caplet_log::{info, install_writer, semihosting::SEMIHOSTING_INSTANCE};
use caplet_macros_exports::{drive_executor_forever, kernel_init, spawn_with_priority};
use caplet_sync::SpinLock;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Simple bump allocator. The MPS2-AN385 has 4 MiB of SRAM at
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
#[caplet_task(priority = 2, stack = 4096, name = "boot")]
async fn boot_task() {
    info!(
        "boot",
        "CapletRT v0.1.0 booting on Cortex-M3 @ QEMU (MPS2-AN385)"
    );
    info!("cap_init", "initializing kernel...");
    info!("cap_init", "done. Boot task running.");
    info!("demo", "hello world");
    info!("sched", "idle -> WFI");

    *SHARED_COUNTER.lock() += 1;
}

/// Second task: counts forever to prove the scheduler is alive.
#[caplet_task(priority = 3, stack = 4096, name = "control")]
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

/// Rust entry point. `cortex-m-rt` calls this from the reset vector
/// after initializing BSS.
#[cortex_m_rt::entry]
fn main() -> ! {
    // Install the semihosting log writer before anything can call info!/debug!/etc.
    install_writer(Some(&SEMIHOSTING_INSTANCE));
    kernel_init();
    spawn_with_priority(boot_task(), 2);
    spawn_with_priority(control_task(), 3);
    drive_executor_forever()
}
