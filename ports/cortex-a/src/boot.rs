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
pub const PL011_BASE: u32 = 0x1000_1000;

/// GICv2 distributor physical address.
pub const GICD_BASE: u32 = 0x1000_0100;
/// GICv2 CPU interface physical address.
pub const GICC_BASE: u32 = 0x1000_0200;

/// Approximate load address of the kernel image (matches the linker
/// script that the board crates are expected to provide).
pub const KERNEL_LOAD_ADDR: u32 = 0x4000_0000;

/// Top of physical RAM on vexpress-a9 with `-m 512M`.
pub const RAM_TOP: u32 = 0x6000_0000;

/// Boot entry point (called from the .S boot vector or from QEMU's
/// default `_start`). Returns Never.
pub fn cortex_a_boot() -> ! {
    // 1. Mask interrupts on the GIC distributor.
    unsafe {
        crate::gic::mask_all();
    }

    // 2. Zero BSS — the linker-script-provided symbols `_sbss` and
    // `_ebss` mark the range. In a real implementation these come
    // from the assembly `boot.s`; here we expose the linker symbols
    // via weak stubs that the board's `build.rs` overrides with the
    // real values.
    unsafe extern "C" {
        static mut _sbss: u8;
        static mut _ebss: u8;
    }
    unsafe {
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
    mmu.map_region(
        KERNEL_LOAD_ADDR,
        KERNEL_LOAD_ADDR,
        (RAM_TOP - KERNEL_LOAD_ADDR) as usize,
        PagePerms::KERNEL_RW,
    );
    mmu.map_region(
        0x1000_0000,
        0x1000_0000,
        0x1000_0000,
        PagePerms::DEVICE,
    );
    unsafe {
        mmu.enable();
    }

    rust_main_wrapper()
}

/// Bridge so that the `asm!` block above has a stable symbol to jump
/// to. The kernel's `kernel_bridge.rs` exposes the entry point.
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn rust_main_wrapper() -> ! {
    #[cfg(target_arch = "arm")]
    unsafe {
        core::arch::asm!(
            "dsb sy",
            "isb",
            "mov sp, {sp}",
            "b {main}",
            sp = in(reg) 0x4020_0000u32,
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

/// Identity-mapped page-table base after `Mmu::enable` has run.
#[no_mangle]
pub static mut MMU_BASE_L1: [u32; 4096] = [0; 4096];

/// Public pointer to the L1 page table used by the boot path.
#[no_mangle]
pub static mut L1_PAGE_TABLE: *mut u32 = unsafe { core::ptr::addr_of_mut!(MMU_BASE_L1) as *mut u32 };
