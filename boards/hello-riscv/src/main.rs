//! Minimal RISC-V board for QEMU `virt` and Renode `riscv_virt`.
//!
//! The architecture port supplies the M-mode `_start`, the M→S
//! transition, BSS clearing and UART initialisation. This board only
//! provides the `rust_main` that the port calls once supervisor mode is
//! established.

#![no_std]
#![no_main]

extern crate alloc;

use caplet_riscv::Uart;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Bump allocator over the linker-provided heap window. The port pulls
/// in `caplet-kernel`, whose `alloc` usage requires a global allocator
/// even for this minimal image.
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

/// Board entry called by `caplet_riscv::riscv_boot`.
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    #[cfg(target_arch = "riscv64")]
    Uart::put_line("CapletRT booting on RISC-V 64 (rv64gc)");
    #[cfg(target_arch = "riscv32")]
    Uart::put_line("CapletRT booting on RISC-V 32 (rv32imac)");

    Uart::put_line("boot: M-mode reset -> PMP -> mret -> S-mode");
    Uart::put_line("uart: NS16550a @ 0x10000000");
    Uart::put_line("clint: 10 MHz @ 0x02000000");
    Uart::put_line("plic: context 1 (hart 0, S-mode)");
    Uart::put_line("hello world");
    Uart::put_line("CapletRT RISC-V boot OK");

    // Hand control to the capability kernel: install the identity map,
    // create two cooperative tasks and exercise the scheduler.  The
    // marker above is printed first so the simulator harness keeps a
    // stable success contract independent of scheduler output.
    caplet_riscv::kernel_bridge::kernel_main()
}
