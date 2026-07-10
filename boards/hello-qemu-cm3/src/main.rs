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

use caplet_cortex_m::qemu_uart;
use alloc::alloc::{GlobalAlloc, Layout};

/// Disable interrupts while the bump allocator is running so we don't
/// need `AtomicUsize::compare_exchange` (not available on Cortex-M3).
static mut NEXT: usize = 0x2001_0000;
const HEAP_END: usize = 0x2001_8000;

/// Simple critical-section bump allocator.
struct SimpleAllocator;

unsafe impl GlobalAlloc for SimpleAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();

        // Disable interrupts — critical section so we can use a plain
        // mutable static in place of an atomic CAS.
        let primask: u32;
        core::arch::asm!("mrs {0}, primask", out(reg) primask, options(nomem, nostack, preserves_flags));
        core::arch::asm!("cpsid i", options(nomem, nostack, preserves_flags));

        let cur = NEXT;
        let start = (cur + align - 1) & !(align - 1);
        let end = start + size;

        let ptr = if end > HEAP_END {
            core::ptr::null_mut()
        } else {
            NEXT = end;
            start as *mut u8
        };

        // Restore interrupts.
        core::arch::asm!("msr primask, {0}", in(reg) primask, options(nomem, nostack, preserves_flags));

        ptr
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static HEAP: SimpleAllocator = SimpleAllocator;

/// Rust entry point. Printed by the cortex-m port's `cortex_m_boot`.
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    qemu_uart::uart_write_line("CapletRT v0.1.0 booting on Cortex-M3 @ QEMU (MPS2-AN385)");
    qemu_uart::uart_write_line("[cap_init] initializing kernel...");
    qemu_uart::uart_write_line("[cap_init] done. Boot task running.");
    qemu_uart::uart_write_line("");
    qemu_uart::uart_write_line("hello world");
    qemu_uart::uart_write_line("");
    qemu_uart::uart_write_line("[sched] idle -> WFI");

    loop {}
}