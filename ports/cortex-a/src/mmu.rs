//! Cortex-A MMU.
//!
//! Implements a one-level page table (4096 × 32-bit section
//! descriptors, 1 MiB granularity) backed by a static array.
//! Sections are encoded according to the ARMv7-A architecture
//! reference manual.
//!
//! docv1.md §13.2 chose this approach: "L1 section descriptor rather
//! than 2-level page table (v1 simplification)" — see the M3
//! architecture ADR.

/// 1 MiB sections, one per L1 entry.
pub const L1_SIZE: usize = 4096;

/// Section descriptor bits (ARMv7-A).
const AP_PRIV_RW: u32 = 0b01 << 10;
const AP_USER_RW: u32 = 0b11 << 10;
const TEX_0_C_0_B_1: u32 = 0b0000_1010; // TEX=0, C=0, B=1 = strongly ordered
const SECTION_BIT: u32 = 1 << 1;
const PAGE_TABLE_PAGE_PERMS: u32 = AP_PRIV_RW | SECTION_BIT;

/// Page permission bits as understood by the kernel core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct APagePerms {
    /// Raw AP bits, etc.
    bits: u32,
}

impl APagePerms {
    /// Construct from the kernel-facing representation.
    pub const fn from_kernel(p: caplet_kernel::mmu::PagePerms) -> Self {
        let mut bits = SECTION_BIT;
        if p.device {
            bits |= TEX_0_C_0_B_1;
        }
        if p.user_read || p.user_write {
            bits |= AP_USER_RW;
            if !p.user_write {
                // Read-only user: AP=0b10.
                bits &= !AP_USER_RW;
                bits |= 0b10 << 10;
            }
        } else {
            bits |= AP_PRIV_RW;
        }
        Self { bits }
    }

    /// Raw bits used to populate the section descriptor.
    pub const fn raw(&self) -> u32 {
        self.bits
    }
}

/// MMU abstraction backed by a static L1 table.
pub struct Mmu {
    /// Static L1 page table.
    l1: [u32; L1_SIZE],
    /// Has the MMU been enabled?
    enabled: bool,
}

impl Mmu {
    /// Construct a new MMU with the L1 table zeroed.
    pub const fn new() -> Self {
        Self {
            l1: [0; L1_SIZE],
            enabled: false,
        }
    }

    /// Map a contiguous physical region starting at `paddr` into the
    /// virtual address space at `vaddr`, covering `size` bytes. Only
    /// page-aligned sizes are accepted.
    pub fn map_region(
        &mut self,
        vaddr: u32,
        paddr: u32,
        size: usize,
        perms: caplet_kernel::mmu::PagePerms,
    ) -> Result<(), MmuError> {
        if size == 0 || (size & 0x000F_FFFF) != 0 {
            return Err(MmuError::BadAlignment);
        }
        if (vaddr & 0x000F_FFFF) != 0 || (paddr & 0x000F_FFFF) != 0 {
            return Err(MmuError::BadAlignment);
        }
        let kernel_perms = APagePerms::from_kernel(perms);
        let mut offset: u32 = 0;
        while (offset as usize) < size {
            let v = vaddr + offset;
            let p = paddr + offset;
            let idx = (v >> 20) as usize;
            if idx >= L1_SIZE {
                return Err(MmuError::OutOfMemory);
            }
            self.l1[idx] = (p & 0xFFF0_0000) | kernel_perms.raw() | SECTION_BIT;
            offset = offset.wrapping_add(0x10_0000);
        }
        Ok(())
    }

    /// Unmap a region by clearing the corresponding L1 entries.
    pub fn unmap_region(&mut self, vaddr: u32, size: usize) -> Result<(), MmuError> {
        if size == 0 {
            return Err(MmuError::BadAlignment);
        }
        let mut offset: u32 = 0;
        while (offset as usize) < size {
            let v = vaddr + offset;
            let idx = (v >> 20) as usize;
            if idx >= L1_SIZE {
                return Err(MmuError::OutOfMemory);
            }
            self.l1[idx] = 0;
            offset = offset.wrapping_add(0x10_0000);
        }
        Ok(())
    }

    /// Enable the MMU by writing TTBR0, DACR, and the SCTLR bits
    /// for MMU + caches. Performs a TLB invalidate VA after.
    pub unsafe fn enable(&mut self) {
        let l1_addr = self.l1.as_ptr() as u32;
        #[cfg(target_arch = "arm")]
        {
            core::arch::asm!(
                // Set TTBR0
                "mcr p15, 0, {ttbr}, c2, c0, 0",
                // Domain 0 = client (0b01)
                "mov r1, #0x1",
                "mcr p15, 0, r1, c3, c0, 0",
                // Invalidate TLB
                "mov r1, #0",
                "mcr p15, 0, r1, c8, c7, 0",
                // Enable MMU + caches
                "mrc p15, 0, r1, c1, c0, 0",
                "orr r1, r1, #0x5", // M + C
                "orr r1, r1, #0x1000", // I (instruction cache)
                "mcr p15, 0, r1, c1, c0, 0",
                "dsb sy",
                "isb",
                ttbr = in(reg) l1_addr,
                out("r1") _,
            );
        }
        #[cfg(not(target_arch = "arm"))]
        {
            // On host we cannot execute the MMU on instructions; we
            // simply mark the MMU as enabled and let the host test
            // harness tick the counter.
            let _ = l1_addr;
        }
        self.enabled = true;
    }

    /// Returns `true` if the MMU has been enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Invalidate the TLB for a single vaddr. SMP callers must
    /// additionally IPI the other CPUs.
    pub fn tlb_invalidate(&mut self, vaddr: u32) {
        #[cfg(target_arch = "arm")]
        unsafe {
            core::arch::asm!(
                "mcr p15, 0, {v}, c8, c7, 1",
                "dsb sy",
                "isb",
                v = in(reg) vaddr,
                options(nostack, preserves_flags)
            );
        }
        #[cfg(not(target_arch = "arm"))]
        {
            let _ = vaddr;
        }
    }
}

impl Default for Mmu {
    fn default() -> Self {
        Self::new()
    }
}

impl caplet_kernel::arch::Arch for Mmu {
    fn name() -> &'static str {
        "cortex-a9"
    }

    fn max_cpus() -> u32 {
        // QEMU vexpress-a9 supports 1-4 cores.
        4
    }

    fn early_init() {
        // Already done in `cortex_a_boot`. The kernel core will
        // not call this twice; the function exists to satisfy
        // the trait.
    }

    unsafe fn context_switch(_prev: *mut u32, _next: *const u32) {
        // Filled in by context.rs at link time.
    }

    unsafe fn start_first_task(_next: *const u32) -> ! {
        // Filled in by context.rs at link time.
        loop {}
    }

    fn now_ticks() -> u64 {
        crate::tick::now_ticks()
    }

    fn request_reschedule() {
        unsafe { crate::gic::sgi_self(0) }
    }

    fn wait_for_interrupt() {
        #[cfg(target_arch = "arm")]
        unsafe {
            core::arch::asm!("wfi", options(nostack, preserves_flags));
        }
        #[cfg(not(target_arch = "arm"))]
        {
            core::hint::spin_loop();
        }
    }
}

/// Internal copy of the kernel-side [`MmuError`] so the port does
/// not have to import `kernel::mmu::MmuError` (which is
/// architecture-independent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmuError {
    /// A region is not page-aligned.
    BadAlignment,
    /// Address would overflow the L1 table.
    OutOfMemory,
}

/// Re-export the kernel-side permission enum for boards.
pub use caplet_kernel::mmu::PagePerms;
