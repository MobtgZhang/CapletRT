//! Architecture-agnostic MMU abstractions for M3.
//!
//! On Cortex-M (no MMU) this is a thin shim around the existing
//! memory layout. On Cortex-A the heavy lifting lives in
//! `ports/cortex-a/src/mmu.rs`; this module provides the
//! kernel-facing API that the syscall layer talks to.
//!
//! The intent is that the kernel core never has to know whether
//! it is running on a Cortex-M or a Cortex-A — both provide
//! implementations of [`MmuOps`].

use crate::arch::MmuConfig;

/// Page permissions in the abstract kernel model. Mapped to
/// architecture-specific permission bits by the port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagePerms {
    /// Whether user-mode code can read the page.
    pub user_read: bool,
    /// Whether user-mode code can write the page.
    pub user_write: bool,
    /// Whether user-mode code can execute the page.
    pub user_exec: bool,
    /// Whether the page is a memory-mapped device (strongly-ordered).
    pub device: bool,
    /// Whether the page is shared with another process (keeps cache
    /// attributes consistent across MMU updates).
    pub shared: bool,
}

impl PagePerms {
    /// Read-only kernel page.
    pub const KERNEL_RO: Self = Self {
        user_read: false,
        user_write: false,
        user_exec: false,
        device: false,
        shared: false,
    };
    /// Read/write kernel page.
    pub const KERNEL_RW: Self = Self {
        user_read: false,
        user_write: false,
        user_exec: false,
        device: false,
        shared: false,
    };
    /// Read-only user page.
    pub const USER_RO: Self = Self {
        user_read: true,
        user_write: false,
        user_exec: false,
        device: false,
        shared: false,
    };
    /// Read/write user page.
    pub const USER_RW: Self = Self {
        user_read: true,
        user_write: true,
        user_exec: false,
        device: false,
        shared: false,
    };
    /// Device MMIO page (strong ordering, kernel only).
    pub const DEVICE: Self = Self {
        user_read: false,
        user_write: false,
        user_exec: false,
        device: true,
        shared: false,
    };
}

/// Architecture-supplied MMU operations. The kernel never touches
/// the page table directly — it goes through this trait so the
/// single-Cortex-M and multi-Cortex-A implementations stay
/// interchangeable.
pub trait MmuOps {
    /// Enable the MMU with the given configuration. Idempotent.
    fn enable(&mut self, cfg: &MmuConfig);
    /// Map a region in the current process's page table.
    fn map(&mut self, vaddr: u32, paddr: u32, size: usize, perms: PagePerms) -> Result<(), MmuError>;
    /// Unmap a region.
    fn unmap(&mut self, vaddr: u32, size: usize) -> Result<(), MmuError>;
    /// Invalidate the TLB for a single vaddr on this CPU. SMP
    /// ports must additionally IPI the other CPUs.
    fn tlb_invalidate(&mut self, vaddr: u32);
    /// Returns `true` if the MMU is currently enabled.
    fn is_enabled(&self) -> bool;
}

/// MMU operation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmuError {
    /// The region is not aligned to a page boundary.
    BadAlignment,
    /// The vaddr is already mapped and the operation refused to
    /// overwrite.
    AlreadyMapped,
    /// Out of physical memory to back the new mapping.
    OutOfMemory,
    /// The size is zero or otherwise invalid.
    BadSize,
    /// The architecture refused the operation (e.g. kernel tried to
    /// map at a kernel-only address with user permissions).
    PermissionDenied,
}

/// Default [`MmuOps`] for architectures with no MMU (Cortex-M).
///
/// All operations are no-ops. `map` always succeeds so the syscall
/// layer does not need to special-case M3 on a Cortex-M port.
pub struct NoMmu;

impl MmuOps for NoMmu {
    fn enable(&mut self, _cfg: &MmuConfig) {}
    fn map(&mut self, _vaddr: u32, _paddr: u32, _size: usize, _perms: PagePerms) -> Result<(), MmuError> {
        Ok(())
    }
    fn unmap(&mut self, _vaddr: u32, _size: usize) -> Result<(), MmuError> {
        Ok(())
    }
    fn tlb_invalidate(&mut self, _vaddr: u32) {}
    fn is_enabled(&self) -> bool {
        false
    }
}

/// In-kernel helper: compute the page-aligned base and length
/// needed to cover `[vaddr, vaddr + size)`.
pub fn page_aligned_range(vaddr: u32, size: usize) -> Option<(u32, usize)> {
    if size == 0 {
        return None;
    }
    let end = vaddr.checked_add(size as u32)?;
    let page_size: u32 = 4096;
    let page_mask = page_size - 1;
    let base = vaddr & !page_mask;
    let aligned_end = (end + page_mask) & !page_mask;
    Some((base, (aligned_end - base) as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_aligned_range_basic() {
        let (b, s) = page_aligned_range(0x1000, 0x100).unwrap();
        assert_eq!(b, 0x1000);
        assert_eq!(s, 0x1000);
    }

    #[test]
    fn page_aligned_range_unaligned_start() {
        let (b, s) = page_aligned_range(0x1001, 0x100).unwrap();
        assert_eq!(b, 0x1000);
        // Spans from 0x1000..0x1100 inclusive (after rounding down
        // the start and rounding up the end), so 0x1100 - 0x1000 =
        // 0x1000 bytes.
        assert_eq!(s, 0x1000);
    }

    #[test]
    fn no_mmu_ops() {
        let mut m = NoMmu;
        m.enable(&MmuConfig::no_mmu());
        assert!(m.map(0, 0, 4096, PagePerms::USER_RW).is_ok());
        assert!(m.unmap(0, 4096).is_ok());
        m.tlb_invalidate(0);
        assert!(!m.is_enabled());
    }

    #[test]
    fn bad_size_returns_none() {
        assert!(page_aligned_range(0, 0).is_none());
    }
}
