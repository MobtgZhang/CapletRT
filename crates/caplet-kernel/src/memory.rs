//! Memory layout definitions

/// Memory region descriptor
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    /// Start address
    pub start: u32,
    /// Size in bytes
    pub size: usize,
    /// Access permissions
    pub permissions: u32,
    /// Region name for debugging
    pub name: &'static str,
}

impl MemoryRegion {
    /// Create a new memory region
    pub const fn new(start: u32, size: usize, permissions: u32, name: &'static str) -> Self {
        Self {
            start,
            size,
            permissions,
            name,
        }
    }

    /// Get the end address
    pub fn end(&self) -> u32 {
        self.start + self.size as u32
    }

    /// Check if an address is in this region
    pub fn contains(&self, addr: u32) -> bool {
        addr >= self.start && addr < self.end()
    }
}

/// Memory layout for the system
#[derive(Debug, Clone, Copy)]
pub struct MemoryLayout {
    /// Flash start address
    pub flash_start: u32,
    /// Flash size
    pub flash_size: usize,
    /// RAM start address
    pub ram_start: u32,
    /// RAM size
    pub ram_size: usize,
    /// Kernel stack size
    pub kernel_stack_size: usize,
    /// Heap start address
    pub heap_start: u32,
    /// Heap size
    pub heap_size: usize,
}

impl MemoryLayout {
    /// Create default memory layout
    pub const fn default() -> Self {
        Self {
            flash_start: 0x0800_0000,
            flash_size: 512 * 1024,
            ram_start: 0x2000_0000,
            ram_size: 128 * 1024,
            kernel_stack_size: 2048,
            heap_start: 0x2001_8000,
            heap_size: 32 * 1024,
        }
    }

    /// Create minimal memory layout
    pub const fn minimal() -> Self {
        Self {
            flash_start: 0x0800_0000,
            flash_size: 64 * 1024,
            ram_start: 0x2000_0000,
            ram_size: 16 * 1024,
            kernel_stack_size: 1024,
            heap_start: 0x2000_4000,
            heap_size: 4 * 1024,
        }
    }

    /// Create QEMU memory layout
    pub const fn qemu() -> Self {
        Self {
            flash_start: 0x0000_0000,
            flash_size: 1024 * 1024,
            ram_start: 0x2000_0000,
            ram_size: 64 * 1024,
            kernel_stack_size: 2048,
            heap_start: 0x2001_0000,
            heap_size: 16 * 1024,
        }
    }

    /// Get RAM end address
    pub fn ram_end(&self) -> u32 {
        self.ram_start + self.ram_size as u32
    }

    /// Get flash end address
    pub fn flash_end(&self) -> u32 {
        self.flash_start + self.flash_size as u32
    }

    /// Get the memory regions for MPU/MMU configuration
    pub fn regions(&self) -> [MemoryRegion; 4] {
        [
            MemoryRegion::new(self.flash_start, self.flash_size, 0, "flash"),
            MemoryRegion::new(self.ram_start, self.ram_size, 0, "ram"),
            MemoryRegion::new(self.heap_start, self.heap_size, 0, "heap"),
            MemoryRegion::new(0, 0, 0, ""),
        ]
    }
}

impl Default for MemoryLayout {
    fn default() -> Self {
        Self::default()
    }
}