//! Memory syscall handlers
//!
//! Provides memory allocation and mapping for tasks:
//! - MapMemory: map a capability into task's address space
//! - UnmapMemory: unmap memory from task's address space
//! - AllocateMemory: allocate heap memory
//! - FreeMemory: free heap memory

use crate::syscall::{Syscall, SyscallArgs, SyscallResult};
use crate::kernel::{with_kernel, CURRENT_TASK_ID};
use crate::bump_allocator;
use core::sync::atomic::{AtomicU32, Ordering};

const ERR_INVALID_ADDR: u32 = 40;
const ERR_OUT_OF_MEMORY: u32 = 41;
const ERR_NOT_MAPPED: u32 = 42;
const ERR_PERMISSION_DENIED: u32 = 43;

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
        Syscall::MapMemory => {
            // arg0 = base address
            // arg1 = size
            // arg2 = flags (r/w/x)
            let base = args.arg0 as usize;
            let size = args.arg1 as usize;
            let flags = args.arg2;

            if base == 0 || size == 0 {
                return SyscallResult::err(ERR_INVALID_ADDR);
            }

            let read = (flags & 1) != 0;
            let write = (flags & 2) != 0;
            let execute = (flags & 4) != 0;

            // TODO: integrate with MPU/MMU
            // For now, just acknowledge the mapping
            SyscallResult::ok(NEXT_REGION_ID.fetch_add(1, Ordering::Relaxed))
        }
        Syscall::UnmapMemory => {
            // arg0 = region id
            let region_id = args.arg0;

            if region_id == 0 {
                return SyscallResult::err(ERR_NOT_MAPPED);
            }

            // TODO: unmap from MPU/MMU
            SyscallResult::ok(0)
        }
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

            // Bump allocator doesn't support free, but we acknowledge the call
            // TODO: switch to a proper allocator that supports free
            SyscallResult::ok(0)
        }
        _ => SyscallResult::err(1),
    }
}
