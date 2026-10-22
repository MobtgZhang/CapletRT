//! PMP (Physical Memory Protection) for RISC-V.
//!
//! The PMP provides hardware-enforced memory protection on RISC-V
//! cores that lack an MMU.  The QEMU/Renode bring-up configures PMP
//! entry 0..3 in the M-mode reset stub in `boot.rs`, before the
//! `mret` that enters supervisor mode.  PMP CSRs are not writable from
//! S-mode, so this module documents the layout but deliberately does
//! not try to reprogram them at runtime.
//!
//! The port uses PMP to enforce:
//!   - Kernel code + data: read + write + execute (0x8000_0000+)
//!   - Device MMIO regions: read + write (no execute)
//!   - Everything else: no access (locked)
//!
//! This module documents the design; the concrete boot-time encoding
//! lives in the `_start` assembly in `boot.rs` (`pmpaddr0..3` and
//! `pmpcfg0`).

/// Number of PMP entries available (minimum 16 on most cores).
pub const PMP_ENTRY_COUNT: usize = 16;

/// PMP address register encoding mode.
#[derive(Debug, Clone, Copy)]
pub enum PmpAddrMode {
    /// TOR — top-of-range (address in pmpaddrN-1).
    Tor,
    /// NAPOT — naturally aligned power-of-two.
    Napot,
    /// NA4 — naturally aligned 4-byte region.
    Na4,
}

/// PMP configuration register entry.
#[derive(Debug, Clone, Copy)]
pub struct PmpCfg {
    /// Which PMP entry this config applies to.
    pub entry: u8,
    /// Read permission.
    pub read: bool,
    /// Write permission.
    pub write: bool,
    /// Execute permission.
    pub exec: bool,
    /// Locked (persists after mret, ignore on W chapters).
    pub lock: bool,
    /// Address mode.
    pub mode: PmpAddrMode,
}

impl PmpCfg {
    /// Kernel code region: RWX, locked.
    ///
    /// Covers the entire QEMU virt DRAM at 0x8000_0000.
    pub fn kernel_code() -> Self {
        Self {
            entry: 0,
            read: true,
            write: true,
            exec: true,
            lock: true,
            mode: PmpAddrMode::Napot,
        }
    }

    /// Device MMIO region: RW, locked.
    pub fn mmio(base: u32, size: u32) -> Self {
        let _ = base;
        let _ = size;
        Self {
            entry: 1,
            read: true,
            write: true,
            exec: false,
            lock: true,
            mode: PmpAddrMode::Tor,
        }
    }
}

// ---------------------------------------------------------------------
// Sv39 / Sv32 identity mapping
// ---------------------------------------------------------------------
//
// The kernel image is linked at 0x8000_0000.  The QEMU virt and Renode
// platforms place CLINT, PLIC and UART below 0x1000_0000.  We therefore
// install an identity map that covers both the device window and the RAM
// window.  Using large leaves keeps the tables tiny (a single page for
// Sv39) and avoids allocating page-table memory at boot.

/// Sv39 PTE valid bit.
pub const PTE_V: u64 = 1 << 0;
/// Sv39 PTE read bit.
pub const PTE_R: u64 = 1 << 1;
/// Sv39 PTE write bit.
pub const PTE_W: u64 = 1 << 2;
/// Sv39 PTE execute bit.
pub const PTE_X: u64 = 1 << 3;
/// Accessed bit; RISC-V hardware may set it, software should set it for
/// systems without an A/D update facility (QEMU accepts either).
pub const PTE_A: u64 = 1 << 6;
/// Dirty bit.
pub const PTE_D: u64 = 1 << 7;

#[cfg(all(target_arch = "riscv64", not(test)))]
#[repr(align(4096))]
struct Sv39Root([u64; 512]);

#[cfg(all(target_arch = "riscv64", not(test)))]
static mut SV39_ROOT: Sv39Root = Sv39Root([0; 512]);

#[cfg(all(target_arch = "riscv32", not(test)))]
#[repr(align(4096))]
struct Sv32Root([u32; 1024]);

#[cfg(all(target_arch = "riscv32", not(test)))]
static mut SV32_ROOT: Sv32Root = Sv32Root([0; 1024]);

/// Enable Sv39 paging with an identity map (RV64 only).
///
/// Returns `true` when paging was installed.  On RV32 and on host builds
/// this is a no-op returning `false`.
#[cfg(all(target_arch = "riscv64", not(test)))]
pub fn enable_sv39() -> bool {
    // SAFETY: the address of a static is always valid; the surrounding
    // block serialises the only writer during single-hart boot.
    let root = unsafe { core::ptr::addr_of_mut!(SV39_ROOT.0) } as *mut u64;
    // SAFETY: the table is a static, page-aligned array and this runs
    // once during single-hart boot with paging disabled.
    unsafe {
        core::ptr::write_bytes(root, 0, 512);
        // [0, 1 GiB): CLINT, PLIC, UART and the PCIe window.  Device
        // memory is read/write but non-executable.
        *root.add(0) = leaf_sv39(0x0000_0000, PTE_V | PTE_R | PTE_W | PTE_A | PTE_D);
        // [2 GiB, 3 GiB): the 128 MiB kernel RAM window at 0x8000_0000.
        *root.add(2) = leaf_sv39(0x8000_0000, PTE_V | PTE_R | PTE_W | PTE_X | PTE_A | PTE_D);
        // satp = MODE(8=Sv39) | ASID(0) | PPN
        let ppn = (root as usize as u64) >> 12;
        let satp = (8u64 << 60) | (ppn & ((1u64 << 44) - 1));
        core::arch::asm!("csrw satp, {satp}", satp = in(reg) satp, options(nostack));
        core::arch::asm!("sfence.vma zero, zero", options(nostack));
    }
    true
}

/// Enable Sv32 paging with an identity map (RV32 only).
#[cfg(all(target_arch = "riscv32", not(test)))]
pub fn enable_sv32() -> bool {
    // SAFETY: see `enable_sv39`.
    let root = unsafe { core::ptr::addr_of_mut!(SV32_ROOT.0) } as *mut u32;
    // SAFETY: see `enable_sv39`.
    unsafe {
        core::ptr::write_bytes(root, 0, 1024);
        let device =
            sv32_leaf(PTE_V as u32 | PTE_R as u32 | PTE_W as u32 | PTE_A as u32 | PTE_D as u32);
        let ram = sv32_leaf(
            PTE_V as u32 | PTE_R as u32 | PTE_W as u32 | PTE_X as u32 | PTE_A as u32 | PTE_D as u32,
        );
        // 4 MiB megapage at the base covers the boot ROM and the first
        // devices.  CLINT (0x0200_0000), PLIC (0x0c00_0000) and UART
        // (0x1000_0000) each get their own 4 MiB leaf.
        *root.add(0x0000_0000 >> 22) = device(0x0000_0000);
        *root.add(0x0200_0000 >> 22) = device(0x0200_0000);
        *root.add(0x0c00_0000 >> 22) = device(0x0c00_0000);
        *root.add(0x1000_0000 >> 22) = device(0x1000_0000);
        // Identity-map the 128 MiB RAM window at 0x8000_0000.
        let mut addr = 0x8000_0000u32;
        while addr < 0x8800_0000 {
            *root.add((addr >> 22) as usize) = ram(addr);
            addr = addr.wrapping_add(4 * 1024 * 1024);
        }
        let satp = ((root as usize as u32) >> 12) | (1u32 << 31);
        core::arch::asm!("csrw satp, {satp}", satp = in(reg) satp, options(nostack));
        core::arch::asm!("sfence.vma zero, zero", options(nostack));
    }
    true
}

#[cfg(all(target_arch = "riscv64", not(test)))]
#[inline]
fn leaf_sv39(pa: u64, flags: u64) -> u64 {
    ((pa >> 12) << 10) | flags
}

#[cfg(all(target_arch = "riscv32", not(test)))]
#[inline]
fn sv32_leaf(flags: u32) -> impl Fn(u32) -> u32 {
    move |pa: u32| ((pa >> 12) << 10) | flags
}

/// Host/RV32 stub (see [`enable_sv39`]).
#[cfg(not(all(target_arch = "riscv64", not(test))))]
pub fn enable_sv39() -> bool {
    false
}

/// Host/RV64 stub (see [`enable_sv32`]).
#[cfg(not(all(target_arch = "riscv32", not(test))))]
pub fn enable_sv32() -> bool {
    false
}

/// Enable the architecture's native paging mode.
pub fn enable_native_paging() -> bool {
    #[cfg(target_arch = "riscv64")]
    {
        return enable_sv39();
    }
    #[cfg(target_arch = "riscv32")]
    {
        return enable_sv32();
    }
    #[allow(unreachable_code)]
    false
}
