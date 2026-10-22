//! Cortex-A9 bring-up board.
//!
//! This board deliberately owns its reset vector and MMU setup so it
//! can run unchanged on both QEMU `vexpress-a9` and the Renode
//! Cortex-A9 platform in `renode/cortex-a9.repl`.
//!
//! Boot contract:
//! * A32 / SVC mode, MMU off, entered at `_reset` in low DRAM.
//! * `_reset` installs the initial stack and calls `boot_main`.
//! * `boot_main` clears BSS, builds an identity-mapped L1 section table,
//!   enables the MMU/caches, initialises PL011, and reports status.

#![no_std]
#![no_main]

extern crate alloc;

use caplet_cortex_a::kernel_bridge::{kernel_main_with_console, KernelConsole};
use core::arch::global_asm;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const PL011_BASE: usize = 0x1000_9000;
const UART_DR: usize = PL011_BASE;
const UART_FR: usize = PL011_BASE + 0x18;
const UART_IBRD: usize = PL011_BASE + 0x24;
const UART_FBRD: usize = PL011_BASE + 0x28;
const UART_LCRH: usize = PL011_BASE + 0x2C;
const UART_CR: usize = PL011_BASE + 0x30;
const FR_TXFF: u32 = 1 << 5;

const L1_ENTRIES: usize = 4096;

/// Heap used by the kernel's `Box` allocations.  It sits between the
/// image/BSS and the kernel boot heap configured by
/// `MemoryLayout::vexpress_a9`.
const HEAP_BASE: usize = 0x6200_0000;
const HEAP_END: usize = 0x6300_0000;

const SECTION: u32 = 0b10;
const DESC_NORMAL: u32 = SECTION | 0x1C00 | 0x0C; // WB cache, full access
const DESC_DEVICE: u32 = SECTION | 0x0C00 | 0x10 | 0x04; // device, XN

#[repr(align(16384))]
struct PageTable([u32; L1_ENTRIES]);

static mut L1_TABLE: PageTable = PageTable([0; L1_ENTRIES]);

const SECONDARY_STACK_SIZE: usize = 32 * 1024;

#[repr(align(16))]
struct SecondaryStack([u8; SECONDARY_STACK_SIZE]);

const SECONDARY_STACK_INIT: SecondaryStack = SecondaryStack([0; SECONDARY_STACK_SIZE]);

static mut SECONDARY_STACKS: [SecondaryStack; 3] = [
    SECONDARY_STACK_INIT,
    SECONDARY_STACK_INIT,
    SECONDARY_STACK_INIT,
];

static CONSOLE_LOCK: AtomicBool = AtomicBool::new(false);

struct ConsoleGuard;

impl ConsoleGuard {
    fn acquire() -> Self {
        while CONSOLE_LOCK
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        Self
    }
}

impl Drop for ConsoleGuard {
    fn drop(&mut self) {
        CONSOLE_LOCK.store(false, Ordering::Release);
    }
}

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
    @ QEMU's vexpress-a9 releases all -smp CPUs at this same reset
    @ vector.  Only the boot CPU runs the bring-up path; secondaries
    @ must not clear BSS or reprogram the shared UART concurrently.
    mrc     p15, 0, r0, c0, c0, 5      @ MPIDR
    and     r0, r0, #0xff              @ Aff0 = CPU id
    cmp     r0, #0
    bne     secondary_mailbox
    ldr     sp, =_estack
    mov     fp, #0
    bl      boot_main
1:  b       1b

secondary_mailbox:
    @ The boot CPU publishes stack/top, shared L1 table and entry point
    @ in the Cortex-A port mailbox.  Each core spins here with its MMU
    @ off until the primary has finished BSS, MMU and UART setup.
    mov     r4, r0
    ldr     r1, =CAPLET_SECONDARY_MAILBOX
    add     r1, r1, r0, lsl #4
1:  ldr     r2, [r1]
    ldr     r3, =0xC0DE0001
    cmp     r2, r3
    bne     1b
    dmb     sy
    ldr     sp, [r1, #4]
    ldr     r12, [r1, #12]
    ldr     r1, [r1, #8]
    mov     r0, r4
    blx     r12
2:  wfi
    b       2b
    .size _reset, . - _reset
    "#
);

fn uart_init() {
    unsafe {
        write_volatile(UART_CR as *mut u32, 0);
        write_volatile(UART_IBRD as *mut u32, 13);
        write_volatile(UART_FBRD as *mut u32, 4);
        write_volatile(UART_LCRH as *mut u32, 0x70);
        write_volatile(UART_CR as *mut u32, 0x301);
    }
}

fn putc(c: u8) {
    unsafe {
        while read_volatile(UART_FR as *const u32) & FR_TXFF != 0 {
            core::hint::spin_loop();
        }
        write_volatile(UART_DR as *mut u32, c as u32);
    }
}

fn puts(s: &str) {
    let _guard = ConsoleGuard::acquire();
    for c in s.bytes() {
        if c == b'\n' {
            putc(b'\r');
        }
        putc(c);
    }
}

fn put_hex(mut value: u32) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    puts("0x");
    let mut started = false;
    for shift in (0..8).rev() {
        let digit = ((value >> (shift * 4)) & 0xF) as usize;
        if digit != 0 || started || shift == 0 {
            started = true;
            putc(HEX[digit]);
        }
    }
    let _ = &mut value;
}

fn kernel_write(text: &str) {
    puts(text);
}

fn puts_u32(prefix: &str, mut value: u32, suffix: &str) {
    let mut digits = [0u8; 10];
    let mut len = 0usize;
    loop {
        digits[len] = b'0' + (value % 10) as u8;
        len += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let _guard = ConsoleGuard::acquire();
    for c in prefix.bytes() {
        putc(c);
    }
    while len != 0 {
        len -= 1;
        putc(digits[len]);
    }
    for c in suffix.bytes() {
        putc(c);
    }
}

unsafe fn zero_bss() {
    unsafe extern "C" {
        static mut _sbss: u8;
        static mut _ebss: u8;
    }
    let start = core::ptr::addr_of_mut!(_sbss) as *mut usize;
    let end = core::ptr::addr_of_mut!(_ebss) as *mut usize;
    let mut p = start;
    while p < end {
        core::ptr::write_volatile(p, 0);
        p = p.add(1);
    }
}

unsafe fn map_section(table: *mut u32, va: usize, pa: usize, desc: u32) {
    let index = (va >> 20) & (L1_ENTRIES - 1);
    core::ptr::write_volatile(table.add(index), (pa as u32 & 0xFFF0_0000) | desc);
}

#[cfg(target_arch = "arm")]
unsafe fn install_mmu(table: *mut u32) {
    // Identity-map the 64 MiB image/RAM window and the device windows
    // used by PL011, the SP804 timer and the GIC.
    for offset in (0..0x0400_0000usize).step_by(0x0010_0000) {
        map_section(
            table,
            0x6000_0000 + offset,
            0x6000_0000 + offset,
            DESC_NORMAL,
        );
    }
    for offset in (0..0x0040_0000usize).step_by(0x0010_0000) {
        map_section(
            table,
            0x1000_0000 + offset,
            0x1000_0000 + offset,
            DESC_DEVICE,
        );
        map_section(
            table,
            0x1E00_0000 + offset,
            0x1E00_0000 + offset,
            DESC_DEVICE,
        );
    }

    core::arch::asm!(
        "dsb sy",
        "mcr p15, 0, {ttbr}, c2, c0, 0", // TTBR0 = table
        "mov r1, #1",
        "mcr p15, 0, r1, c3, c0, 0",     // domain 0 = client
        "mcr p15, 0, r1, c8, c7, 0",     // invalidate unified TLB
        "dsb sy",
        "isb",
        "mrc p15, 0, r1, c1, c0, 0",
        "orr r1, r1, #(1 << 0)",          // MMU
        "orr r1, r1, #(1 << 2)",          // D-cache
        "orr r1, r1, #(1 << 12)",         // I-cache
        "mcr p15, 0, r1, c1, c0, 0",
        "dsb sy",
        "isb",
        ttbr = in(reg) table as u32,
        out("r1") _,
        options(nostack)
    );
}

#[cfg(target_arch = "arm")]
#[no_mangle]
pub extern "C" fn boot_main() -> ! {
    unsafe {
        zero_bss();
        let table = core::ptr::addr_of_mut!(L1_TABLE.0) as *mut u32;
        install_mmu(table);
    }

    uart_init();
    puts("CapletRT booting on Cortex-A9 (QEMU/Renode vexpress-a9)\n");
    puts("boot: A32 SVC mode, MMU off -> on\n");
    puts("mmu: L1 section table @ ");
    put_hex(unsafe { core::ptr::addr_of!(L1_TABLE.0) as u32 });
    puts("\n");
    puts("uart: PL011 @ 0x10009000\n");
    puts("hello world\n");
    puts("CapletRT Cortex-A9 boot OK\n");

    caplet_cortex_a::smp::install_kernel_hooks();
    let table = unsafe { core::ptr::addr_of!(L1_TABLE.0) as u32 };
    for cpu in 1..4u32 {
        let stack_top = unsafe {
            core::ptr::addr_of_mut!(SECONDARY_STACKS[(cpu - 1) as usize].0) as usize
                + SECONDARY_STACK_SIZE
        } as u32;
        caplet_cortex_a::smp::prepare_secondary(
            cpu,
            caplet_cortex_a::smp::secondary_entry as *const () as usize as u32,
            stack_top,
            table,
        );
    }
    puts("smp: releasing secondaries\n");
    let online = caplet_kernel::smp::boot_secondary_cpus(4);
    puts_u32("smp: online cpus=", online, "\n");
    let workers = caplet_kernel::smp::wait_for_secondary_workers(caplet_kernel::smp::online_mask());
    puts_u32("smp: worker tasks done=", workers, "\n");

    kernel_main_with_console(KernelConsole::new(kernel_write, "Cortex-A9"))
}

/// Called by the Cortex-A port after a secondary has installed the
/// shared MMU map and its private stack.
#[cfg(target_arch = "arm")]
#[no_mangle]
pub extern "C" fn caplet_secondary_rust(cpu: u32) -> ! {
    match cpu {
        1 => puts("smp: cpu1 online\n"),
        2 => puts("smp: cpu2 online\n"),
        3 => puts("smp: cpu3 online\n"),
        _ => puts("smp: unexpected cpu online\n"),
    }
    caplet_kernel::smp::mark_secondary_online(cpu);
    caplet_kernel::smp::run_secondary_worker(cpu);
    match cpu {
        1 => puts("smp: cpu1 worker done\n"),
        2 => puts("smp: cpu2 worker done\n"),
        3 => puts("smp: cpu3 worker done\n"),
        _ => puts("smp: unexpected cpu worker done\n"),
    }
    loop {
        unsafe { core::arch::asm!("wfi", options(nomem, nostack, preserves_flags)) };
    }
}

/// Host stub so an all-features test build can still compile this board.
#[cfg(not(target_arch = "arm"))]
#[no_mangle]
pub extern "C" fn boot_main() -> ! {
    panic!("hello-cortex-a9 is a bare-metal ARM board")
}

#[cfg(all(target_os = "none", not(test)))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    puts("PANIC on Cortex-A9\n");
    loop {
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
    }
}
