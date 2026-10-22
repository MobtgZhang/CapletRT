//! AArch64 boot entry.
//!
//! QEMU virt enters the ELF at `_start` in EL1h with the MMU off. The
//! reset stub installs the initial stack, then calls [`aarch64_boot`],
//! which clears BSS, initialises the PL011 and dispatches to the board's
//! `rust_main`.

/// Base of DRAM on QEMU virt.
pub const DRAM_BASE: usize = 0x4000_0000;
/// Size of the default 128 MiB DRAM window.
pub const DRAM_SIZE: usize = 0x0800_0000;
/// PL011 UART0 base.
pub const UART0_BASE: usize = 0x0900_0000;

#[cfg(target_arch = "aarch64")]
use crate::uart::Uart;

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text.entry, "ax"
    .balign 16
    .global _start
    .type _start, %function
_start:
    ldr x0, =_estack
    mov sp, x0
    /* Enable FP/AdvSIMD at EL1 and EL0 (CPACR_EL1.FPEN = 0b11).
     * Rust's core library may emit NEON even in otherwise integer
     * code, and the reset value traps those instructions. */
    mrs x1, cpacr_el1
    orr x1, x1, #(3 << 20)
    msr cpacr_el1, x1
    isb
    bl  aarch64_boot
1:  wfi
    b   1b
    .size _start, . - _start

    /* AArch64 exception vector table.  Every entry branches to a small
     * stub which publishes the vector index and calls the Rust handler.
     * The handler never returns: it prints ESR/FAR/ELR/SPSR and parks
     * the CPU, so a fault can no longer silently jump into an
     * undefined-instruction loop. */
    .section .text.vectors, "ax"
    .balign 2048
    .global _vector_table
    .type _vector_table, %function
_vector_table:
    b vec_0
    b vec_1
    b vec_2
    b vec_3
    b vec_4
    b vec_5
    b vec_6
    b vec_7
    b vec_8
    b vec_9
    b vec_10
    b vec_11
    b vec_12
    b vec_13
    b vec_14
    b vec_15

vec_0:  mov x5, #0;  b vec_common
vec_1:  mov x5, #1;  b vec_common
vec_2:  mov x5, #2;  b vec_common
vec_3:  mov x5, #3;  b vec_common
vec_4:  mov x5, #4;  b vec_common
vec_5:  mov x5, #5;  b vec_common
vec_6:  mov x5, #6;  b vec_common
vec_7:  mov x5, #7;  b vec_common
vec_8:  mov x5, #8;  b vec_common
vec_9:  mov x5, #9;  b vec_common
vec_10: mov x5, #10; b vec_common
vec_11: mov x5, #11; b vec_common
vec_12: mov x5, #12; b vec_common
vec_13: mov x5, #13; b vec_common
vec_14: mov x5, #14; b vec_common
vec_15: mov x5, #15; b vec_common

vec_common:
    mrs x1, esr_el1
    mrs x2, far_el1
    mrs x3, elr_el1
    mrs x4, spsr_el1
    mov x0, x5
    bl  aarch64_exception_handler
2:  wfi
    b   2b
    .size _vector_table, . - _vector_table
    "#
);

/// Install the exception vector table in `VBAR_EL1`.
#[cfg(target_arch = "aarch64")]
pub fn install_exception_vectors() {
    unsafe extern "C" {
        static _vector_table: u8;
    }
    let vector_table = core::ptr::addr_of!(_vector_table) as u64;
    // SAFETY: `_vector_table` is 2 KiB aligned and contains 16 valid
    // entries, as required by VBAR_EL1.  Boot is single-threaded here.
    unsafe {
        core::arch::asm!(
            "msr vbar_el1, {vector_table}",
            "isb",
            vector_table = in(reg) vector_table,
            options(nomem, nostack),
        );
    }
}

/// Fatal exception reporter for AArch64.
///
/// Vector index bits are laid out as `group * 4 + kind`, where kind is
/// 0=synchronous, 1=IRQ, 2=FIQ and 3=SError.
#[cfg(target_arch = "aarch64")]
#[no_mangle]
pub extern "C" fn aarch64_exception_handler(
    vector: u64,
    esr: u64,
    far: u64,
    elr: u64,
    spsr: u64,
) -> ! {
    let kind = match vector & 3 {
        0 => "synchronous",
        1 => "IRQ",
        2 => "FIQ",
        _ => "SError",
    };
    let source = match vector >> 2 {
        0 => "current EL/SP0",
        1 => "current EL/SPx",
        2 => "lower EL/AArch64",
        _ => "lower EL/AArch32",
    };
    Uart::put_str("EXCEPTION: ");
    Uart::put_str(source);
    Uart::put_char(b'/');
    Uart::put_str(kind);
    Uart::put_str(" vector=");
    Uart::put_hex(vector);
    Uart::put_str(" ec=");
    Uart::put_hex(esr >> 26);
    Uart::put_str(" esr=");
    Uart::put_hex(esr);
    Uart::put_str(" far=");
    Uart::put_hex(far);
    Uart::put_str(" elr=");
    Uart::put_hex(elr);
    Uart::put_str(" spsr=");
    Uart::put_hex(spsr);
    Uart::put_line("");
    loop {
        // SAFETY: WFE is a low-power hint and this handler never returns.
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)) };
    }
}

/// Rust boot path called by `_start`.
#[cfg(target_arch = "aarch64")]
#[no_mangle]
pub fn aarch64_boot() -> ! {
    zero_bss();
    Uart::init();
    install_exception_vectors();
    Uart::put_line("CapletRT: AArch64 EL1 entry");
    unsafe extern "C" {
        fn rust_main() -> !;
    }
    // SAFETY: the board supplies `rust_main`, which never returns.
    unsafe { rust_main() }
}

/// Clear the `.bss` region described by the linker script.
#[cfg(target_arch = "aarch64")]
pub fn zero_bss() {
    unsafe extern "C" {
        static mut _sbss: u8;
        static mut _ebss: u8;
    }
    // SAFETY: the linker-provided BSS range is writable.
    unsafe {
        let start = core::ptr::addr_of_mut!(_sbss);
        let end = core::ptr::addr_of_mut!(_ebss);
        if end > start {
            core::ptr::write_bytes(start, 0, end.offset_from(start) as usize);
        }
    }
}

/// Host stub.
#[cfg(not(target_arch = "aarch64"))]
pub fn aarch64_boot() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Host stub.
#[cfg(not(target_arch = "aarch64"))]
pub fn zero_bss() {}
