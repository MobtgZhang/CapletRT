//! Module memory allocator.
//!
//! The loader pulls physical pages for a new module from a static
//! pool. The default implementation allocates bump-style from a
//! caller-supplied base; production builds can swap in a real page
//! allocator without changing the public API.

extern crate alloc;

/// Static memory pool description. Boards configure this at boot.
#[derive(Debug, Clone, Copy)]
pub struct ModulePool {
    /// Base address (must be page-aligned).
    pub base: u32,
    /// Total size of the pool in bytes.
    pub size: u32,
    /// Next free offset from `base`.
    next: u32,
}

impl ModulePool {
    /// Construct a pool with `base..base+size` available.
    pub const fn new(base: u32, size: u32) -> Self {
        Self { base, size, next: 0 }
    }

    /// Allocate `size` bytes. Returns `(vaddr, paddr)` or `None` if
    /// the pool is exhausted.
    pub fn alloc(&mut self, size: u32) -> Option<(u32, u32)> {
        // Round up to a 64 KiB alignment so the MMU can map it.
        let aligned = (size + 0xFFFF) & !0xFFFF;
        if self.next + aligned > self.size {
            return None;
        }
        let vaddr = self.base + self.next;
        self.next += aligned;
        Some((vaddr, vaddr))
    }

    /// Reset the pool. Only valid in tests.
    pub fn reset(&mut self) {
        self.next = 0;
    }

    /// Total bytes allocated so far.
    pub fn used(&self) -> u32 {
        self.next
    }

    /// Remaining capacity.
    pub fn free(&self) -> u32 {
        self.size - self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_bump() {
        let mut pool = ModulePool::new(0x6000_0000, 1024 * 1024);
        let (v1, p1) = pool.alloc(0x1000).unwrap();
        let (v2, p2) = pool.alloc(0x1000).unwrap();
        assert_eq!(p1, 0x6000_0000);
        assert_eq!(v1, 0x6000_0000);
        assert_eq!(v2, 0x6001_0000);
        assert_eq!(p2, 0x6001_0000);
    }

    #[test]
    fn exhausts_pool() {
        let mut pool = ModulePool::new(0x7000_0000, 0x10000);
        // 64 KiB pool, first allocation is rounded up to 64 KiB.
        let _ = pool.alloc(1).unwrap();
        assert!(pool.alloc(1).is_none());
    }

    #[test]
    fn reset_clears() {
        let mut pool = ModulePool::new(0x8000_0000, 0x10000);
        let _ = pool.alloc(0x1000).unwrap();
        pool.reset();
        assert_eq!(pool.used(), 0);
    }
}