//! Minimal AArch64 board.
//!
//! The port owns `_start` and the EL1 boot sequence; this board only
//! prints the bring-up report and parks the CPU.

#![no_std]
#![no_main]

extern crate alloc;

use caplet_aarch64::{tick, Uart};
use core::sync::atomic::{AtomicUsize, Ordering};

struct BumpAlloc;

unsafe extern "C" {
    static _sheap: u8;
    static _eheap: u8;
}

static HEAP_NEXT: AtomicUsize = AtomicUsize::new(0);

unsafe impl core::alloc::GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let start = core::ptr::addr_of!(_sheap) as usize;
        let end = core::ptr::addr_of!(_eheap) as usize;
        let mut cur = HEAP_NEXT.load(Ordering::Relaxed);
        loop {
            let base = if cur == 0 { start } else { cur };
            let aligned = (base + layout.align() - 1) & !(layout.align() - 1);
            let next = match aligned.checked_add(layout.size()) {
                Some(next) if next <= end => next,
                _ => return core::ptr::null_mut(),
            };
            match HEAP_NEXT.compare_exchange(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return aligned as *mut u8,
                Err(observed) => cur = observed,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

#[global_allocator]
static ALLOCATOR: BumpAlloc = BumpAlloc;

/// Board entry called by `caplet_aarch64::aarch64_boot`.
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    Uart::put_line("CapletRT booting on AArch64 (Cortex-A53, EL1)");
    Uart::put_line("boot: EL1h, MMU off, identity physical access");
    Uart::put_line("uart: PL011 @ 0x09000000");
    Uart::put_str("timer: CNTFRQ_EL0 = ");
    Uart::put_hex(tick::frequency());
    Uart::put_line(" Hz");
    Uart::put_str("timer: CNTPCT_EL0 = ");
    Uart::put_hex(tick::now_ticks());
    Uart::put_line("");
    Uart::put_line("hello world");
    Uart::put_line("CapletRT AArch64 boot OK");

    caplet_aarch64::kernel_bridge::kernel_main()
}
