//! AArch64 stage-1 MMU support.
//!
//! The port uses a 4 KiB granule and identity maps the first 4 GiB of
//! the physical address space with 1 GiB blocks.  The first gigabyte
//! contains the QEMU/Renode GIC and PL011 UART and is therefore mapped
//! as Device-nGnRnE, non-executable memory.  The second gigabyte holds
//! the 128 MiB DRAM window used by the kernel and is mapped as normal
//! write-back, write-allocate memory.

/// Translation granule used by the port.
pub const PAGE_SIZE: usize = 4096;
/// Number of entries in an AArch64 level-1 translation table.
pub const L1_ENTRIES: usize = 512;
/// AArch64 1 GiB block descriptor.
pub const BLOCK_SIZE: usize = 1024 * 1024 * 1024;

const DESC_VALID: u64 = 1 << 0;
const DESC_ATTR_DEVICE: u64 = 0 << 2;
const DESC_ATTR_NORMAL: u64 = 1 << 2;
const DESC_SH_INNER: u64 = 3 << 8;
const DESC_AF: u64 = 1 << 10;
const DESC_PXN: u64 = 1 << 53;
const DESC_UXN: u64 = 1 << 54;

#[cfg(target_arch = "aarch64")]
const MAIR_DEVICE_NGNRNE: u64 = 0x00;
#[cfg(target_arch = "aarch64")]
const MAIR_NORMAL_WBWA: u64 = 0x44;

#[cfg(target_arch = "aarch64")]
const SCTLR_M: u64 = 1 << 0;
#[cfg(target_arch = "aarch64")]
const SCTLR_C: u64 = 1 << 2;
#[cfg(target_arch = "aarch64")]
const SCTLR_I: u64 = 1 << 12;
#[cfg(target_arch = "aarch64")]
const SCTLR_WXN: u64 = 1 << 19;

#[repr(align(4096))]
struct L1Table([u64; L1_ENTRIES]);

static mut L1_TABLE: L1Table = L1Table([0; L1_ENTRIES]);

/// Return the physical address of the process-wide L1 table.
pub fn l1_table_ptr() -> *mut u64 {
    // SAFETY: taking the address of a mutable static does not create a
    // reference and is valid before and after the MMU is enabled.
    unsafe { core::ptr::addr_of_mut!(L1_TABLE.0) as *mut u64 }
}

const fn block_descriptor(addr: u64, attr: u64, executable: bool) -> u64 {
    let mut desc = (addr & !(BLOCK_SIZE as u64 - 1)) | DESC_VALID | attr | DESC_SH_INNER | DESC_AF;
    if !executable {
        desc |= DESC_PXN | DESC_UXN;
    }
    desc
}

/// Populate the identity map used by every supported AArch64 board.
///
/// This function is safe to call with the MMU disabled and is
/// intentionally idempotent so a board may prepare the tables before
/// handing control to the kernel.
pub fn init_identity_map() {
    let table = l1_table_ptr();
    for index in 0..L1_ENTRIES {
        // SAFETY: L1_TABLE has exactly L1_ENTRIES entries.
        unsafe { core::ptr::write_volatile(table.add(index), 0) };
    }

    // 0x0000_0000..0x4000_0000: GIC, PL011 and other platform MMIO.
    // SAFETY: index 0 is inside the table.
    unsafe {
        core::ptr::write_volatile(table, block_descriptor(0, DESC_ATTR_DEVICE, false));
    }
    for index in 1..L1_ENTRIES {
        // SAFETY: index is within the table and each address is 1 GiB
        // aligned.  DRAM (0x4000_0000) is index 1.
        unsafe {
            core::ptr::write_volatile(
                table.add(index),
                block_descriptor((index as u64) << 30, DESC_ATTR_NORMAL, true),
            );
        }
    }
}

/// Enable stage-1 translation using the prepared identity map.
///
/// # Safety
/// The caller must have called [`init_identity_map`] while executing at
/// EL1, and every address currently in use by the caller must be
/// covered by the identity map.
pub unsafe fn enable_native_paging() {
    init_identity_map();
    #[cfg(target_arch = "aarch64")]
    let ttbr0 = l1_table_ptr() as u64;
    #[cfg(target_arch = "aarch64")]
    let mair = MAIR_DEVICE_NGNRNE | (MAIR_NORMAL_WBWA << 8);
    // T0SZ=32 (4 GiB VA), inner/outer WB-WA, inner-shareable,
    // 4 KiB granule.  TTBR1 walks are disabled because the port uses a
    // single identity-mapped address space.
    #[cfg(target_arch = "aarch64")]
    let tcr = 32u64 | (1 << 8) | (1 << 10) | (3 << 12) | (1 << 23);

    #[cfg(target_arch = "aarch64")]
    {
        core::arch::asm!(
            "msr mair_el1, {mair}",
            "msr tcr_el1, {tcr}",
            "msr ttbr0_el1, {ttbr0}",
            "dsb sy",
            "isb",
            "tlbi vmalle1",
            "dsb sy",
            "isb",
            mair = in(reg) mair,
            tcr = in(reg) tcr,
            ttbr0 = in(reg) ttbr0,
            options(nostack),
        );

        let mut sctlr: u64;
        core::arch::asm!("mrs {0}, sctlr_el1", out(reg) sctlr, options(nomem, nostack));
        sctlr &= !SCTLR_WXN;
        sctlr |= SCTLR_M | SCTLR_C | SCTLR_I;
        core::arch::asm!(
            "msr sctlr_el1, {sctlr}",
            "dsb sy",
            "isb",
            sctlr = in(reg) sctlr,
            options(nostack),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_encoding() {
        let normal = block_descriptor(0x4000_0000, DESC_ATTR_NORMAL, true);
        assert_eq!(normal & 0x3, 1);
        assert_eq!(normal & (3 << 8), DESC_SH_INNER);
        assert_ne!(normal & DESC_AF, 0);
        assert_eq!(normal & (DESC_PXN | DESC_UXN), 0);
        assert_eq!(normal & 0x0000_ffff_c000_0000, 0x4000_0000);

        let device = block_descriptor(0, DESC_ATTR_DEVICE, false);
        assert_eq!(device & (DESC_PXN | DESC_UXN), DESC_PXN | DESC_UXN);
    }

    #[test]
    fn identity_map_covers_dram() {
        assert_eq!(0x4000_0000usize / BLOCK_SIZE, 1);
        assert_eq!(crate::boot::DRAM_BASE / BLOCK_SIZE, 1);
    }
}
