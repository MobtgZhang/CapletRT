//! Memory syscall handlers
//!
//! Provides memory allocation and mapping for tasks:
//! - MapMemory: map a capability into task's address space
//! - UnmapMemory: unmap memory from task's address space
//! - AllocateMemory: allocate heap memory
//! - FreeMemory: free heap memory

use crate::bump_allocator;
use crate::syscall::{Syscall, SyscallArgs, SyscallResult};
use core::sync::atomic::{AtomicU32, Ordering};

const ERR_INVALID_ADDR: u32 = 40;
const ERR_OUT_OF_MEMORY: u32 = 41;
const ERR_NOT_SUPPORTED: u32 = 44;

/// Next memory region ID
static NEXT_REGION_ID: AtomicU32 = AtomicU32::new(1);

/// Memory region descriptor
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// Region ID
    pub id: u32,
    /// Base address
    pub base: usize,
    /// Size in bytes
    pub size: usize,
    /// Whether the region is mapped
    pub mapped: bool,
    /// Read permission
    pub read: bool,
    /// Write permission
    pub write: bool,
    /// Execute permission
    pub execute: bool,
}

impl MemoryRegion {
    /// Create a new memory region descriptor
    pub fn new(base: usize, size: usize) -> Self {
        Self {
            id: NEXT_REGION_ID.fetch_add(1, Ordering::Relaxed),
            base,
            size,
            mapped: false,
            read: true,
            write: true,
            execute: false,
        }
    }
}

/// Handle memory-related syscalls
pub fn handle_memory_syscall(args: SyscallArgs) -> SyscallResult {
    match Syscall::from_number(args.number) {
        // MapMemory/UnmapMemory are routed to `syscall_cap` because
        // they are capability operations with a (vaddr, paddr, size,
        // perms) ABI. Keep them explicitly unsupported here.
        Syscall::MapMemory | Syscall::UnmapMemory => SyscallResult::err(ERR_NOT_SUPPORTED),
        Syscall::AllocateMemory => {
            // arg0 = size
            // arg1 = alignment
            let size = args.arg0 as usize;
            let align = args.arg1.max(1) as usize;

            if size == 0 {
                return SyscallResult::err(ERR_INVALID_ADDR);
            }

            match bump_allocator::bump_allocate(size, align) {
                Some(addr) => SyscallResult::ok(addr as u32),
                None => SyscallResult::err(ERR_OUT_OF_MEMORY),
            }
        }
        Syscall::FreeMemory => {
            // arg0 = address
            let addr = args.arg0 as usize;

            if addr == 0 {
                return SyscallResult::err(ERR_INVALID_ADDR);
            }

            // A bump allocator cannot honour `free`. Returning success
            // here would silently leak and lie to callers, so report
            // the limitation explicitly.
            SyscallResult::err(ERR_NOT_SUPPORTED)
        }
        _ => SyscallResult::err(1),
    }
}
