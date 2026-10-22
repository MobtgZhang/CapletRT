//! Boot path for Cortex-A.
//!
//! The boot flow differs significantly from Cortex-M because QEMU
//! starts the CPU in SVC mode at EL1 with the MMU disabled. This
//! file sets up:
//!
//! 1. Disable all interrupts at the GIC distributor.
//! 2. Zero the BSS region.
//! 3. Build an identity-mapped L1 page table covering the kernel
//!    image plus all device memory regions.
//! 4. Enable the MMU and caches.
//! 5. Issue a barrier, then jump to `rust_main`.

extern crate alloc;

use caplet_kernel::mmu::PagePerms;

use crate::mmu::Mmu;

/// PL011 UART base address on the vexpress-a9 board.
pub const PL011_BASE: u32 = 0x1000_9000;

/// GICv2 distributor physical address.
pub const GICD_BASE: u32 = 0x1E00_1000;
/// GICv2 CPU interface physical address.
pub const GICC_BASE: u32 = 0x1E00_0100;

/// Approximate load address of the kernel image (matches the linker
/// script that the board crates are expected to provide).
pub const KERNEL_LOAD_ADDR: u32 = 0x6000_0000;

/// End of the 64 MiB low-DRAM window used by the link scripts.
pub const RAM_TOP: u32 = 0x6400_0000;

/// Boot entry point (called from the .S boot vector or from QEMU's
/// default `_start`). Returns Never.
pub fn cortex_a_boot() -> ! {
    // 1. Mask interrupts on the GIC distributor.
    unsafe {
        crate::gic::mask_all();
    }

    // 2. Zero BSS — the linker-script-provided symbols `_sbss` and
    // `_ebss` mark the range. These symbols only exist in a bare-metal
    // link (supplied by the board linker script), so this step is
    // compiled only for bare-metal ARM. On host builds (`cargo test`,
    // which links `std`) there is no BSS range to clear and the
    // symbols are undefined, which would fail at link time (LNK2019).
    #[cfg(all(target_arch = "arm", not(test)))]
    unsafe {
        unsafe extern "C" {
            static mut _sbss: u8;
            static mut _ebss: u8;
        }
        let start = core::ptr::addr_of_mut!(_sbss) as *mut u32;
        let end = core::ptr::addr_of_mut!(_ebss) as *mut u32;
        if end > start {
            let mut p = start;
            while p < end {
                core::ptr::write_volatile(p, 0);
                p = p.add(1);
            }
        }
    }

    // 3+4. Set up the MMU with a minimal identity map.
    let mut mmu = Mmu::new();
    publish_l1_table();
    if mmu
        .map_region(
            KERNEL_LOAD_ADDR,
            KERNEL_LOAD_ADDR,
            (RAM_TOP - KERNEL_LOAD_ADDR) as usize,
            PagePerms::KERNEL_RW,
        )
        .is_err()
    {
        boot_failure();
    }
    if mmu
        .map_region(0x1000_0000, 0x1000_0000, 0x1000_0000, PagePerms::DEVICE)
        .is_err()
    {
        boot_failure();
    }
    unsafe {
        mmu.enable();
    }

    rust_main_wrapper()
}

fn boot_failure() -> ! {
    loop {
        #[cfg(target_arch = "arm")]
        unsafe {
            core::arch::asm!("wfi", options(nostack, preserves_flags));
        }
        #[cfg(not(target_arch = "arm"))]
        core::hint::spin_loop();
    }
}

/// Bridge so that the `asm!` block above has a stable symbol to jump
/// to. The kernel's `kernel_bridge.rs` exposes the entry point.
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn rust_main_wrapper() -> ! {
    #[cfg(target_arch = "arm")]
    unsafe {
        unsafe extern "C" {
            static _estack: u8;
        }
        core::arch::asm!(
            "dsb sy",
            "isb",
            "mov sp, {sp}",
            "b {main}",
            sp = in(reg) core::ptr::addr_of!(_estack) as u32,
            main = sym rust_main_bridge,
            options(noreturn)
        );
    }
    #[cfg(not(target_arch = "arm"))]
    loop {
        core::hint::spin_loop();
    }
}

/// On ARM targets the asm block above jumps to this function. On
/// host builds we simply forward to the kernel.
#[inline(never)]
pub extern "C" fn rust_main_bridge() -> ! {
    crate::kernel_bridge::rust_main()
}

/// Public pointer to the L1 page table used by the boot path.
///
/// The table itself lives in `mmu.rs` so that it is 16 KiB aligned and
/// is not overwritten when the boot stack is reused.
#[no_mangle]
pub static mut L1_PAGE_TABLE: *mut u32 = core::ptr::null_mut();

/// Publish the address of the static L1 table for debuggers and boards.
pub fn publish_l1_table() {
    unsafe {
        L1_PAGE_TABLE = crate::mmu::l1_table_ptr();
    }
}
