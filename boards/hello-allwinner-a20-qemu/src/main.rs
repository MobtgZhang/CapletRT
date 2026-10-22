//! Allwinner A20 (Cortex-A8) bring-up board.
//!
//! Runs on QEMU's `cubieboard` machine and on Renode's `a20.repl`
//! platform. Both model the real sun7i UART0 at `0x01C2_8000`, so the
//! console path is the same as on hardware (no semihosting).
//!
//! Boot contract: A32/SVC, MMU off, entered at `_reset` in DRAM. The
//! linker script places the image at `0x6000_0000`, inside the A20's
//! 1 GiB DRAM window.

#![no_std]
#![no_main]

extern crate alloc;

use caplet_allwinner::uart::Ns16550Uart;
use caplet_cortex_a::kernel_bridge::{kernel_main_with_console, KernelConsole};
use core::arch::global_asm;
use core::sync::atomic::{AtomicUsize, Ordering};

/// UART0 console shared by the boot path and the panic handler.
static CONSOLE: Ns16550Uart = Ns16550Uart::uart0();

/// Heap used by the capability kernel.  The A20 has DRAM starting at
/// 0x4000_0000; this image is linked at 0x6000_0000, so the allocator
/// and the kernel boot heap occupy otherwise unused low-RAM pages.
const HEAP_BASE: usize = 0x6200_0000;
const HEAP_END: usize = 0x6300_0000;

struct BumpAlloc;

static HEAP_NEXT: AtomicUsize = AtomicUsize::new(HEAP_BASE);

unsafe impl core::alloc::GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut current = HEAP_NEXT.load(Ordering::Relaxed);
        loop {
            let Some(aligned) = current
                .checked_add(layout.align() - 1)
                .map(|value| value & !(layout.align() - 1))
            else {
                return core::ptr::null_mut();
            };
            let Some(end) = aligned.checked_add(layout.size()) else {
                return core::ptr::null_mut();
            };
            if end > HEAP_END {
                return core::ptr::null_mut();
            }
            match HEAP_NEXT.compare_exchange(current, end, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return aligned as *mut u8,
                Err(observed) => current = observed,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

#[global_allocator]
static ALLOCATOR: BumpAlloc = BumpAlloc;

#[cfg(target_arch = "arm")]
global_asm!(
    r#"
    .syntax unified
    .arm

    .section .text.boot, "ax"
    .global _reset
    .type _reset, %function
_reset:
    mrc     p15, 0, r0, c0, c0, 5      @ MPIDR
    and     r0, r0, #0xff
    cmp     r0, #0
    bne     secondary_park
    ldr     sp, =_estack
    mov     fp, #0
    bl      boot_main
1:  b       1b

secondary_park:
2:  wfi
    b       2b
    .size _reset, . - _reset
    "#
);

fn kernel_write(text: &str) {
    CONSOLE.puts(text);
}

/// Rust entry point called from `_reset` with a valid stack.
#[cfg(target_arch = "arm")]
#[no_mangle]
pub extern "C" fn boot_main() -> ! {
    CONSOLE.init();
    CONSOLE.puts("CapletRT booting on Allwinner A20 (Cortex-A8)\n");
    CONSOLE.puts("uart: sun7i NS16550 UART0 @ 0x01C28000\n");
    CONSOLE.puts("hello world\n");
    CONSOLE.puts("CapletRT Allwinner A20 boot OK\n");

    kernel_main_with_console(KernelConsole::new(kernel_write, "Allwinner A20"))
}

/// Host stub so an all-features test build can still compile this board.
#[cfg(not(target_arch = "arm"))]
#[no_mangle]
pub extern "C" fn boot_main() -> ! {
    panic!("hello-allwinner-a20-qemu is a bare-metal ARM board")
}

#[cfg(all(target_os = "none", not(test)))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    CONSOLE.puts("PANIC on Allwinner A20\n");
    loop {
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
    }
}
