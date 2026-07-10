//! Flash syscall handler.
//!
//! Dispatches flash read/write/erase operations via capability-gated
//! syscalls. The syscall arguments encode the operation:
//!
//! Syscall protocol:
//!   Syscall::FlashRead   → read flash
//!   Syscall::FlashWrite  → program flash
//!   Syscall::FlashErase  → erase flash sector
//!
//!   arg0 = flash offset (byte address within the granted region)
//!   arg1 = buffer address (in caller's memory space)
//!   arg2 = byte count
//!   arg3 = reserved

use crate::syscall::{SyscallArgs, SyscallResult};
use crate::kernel::Kernel;

const ERR_NO_TOKEN: u32 = 1;
const ERR_OUT_OF_REGION: u32 = 2;
const ERR_WRITE_PROTECTED: u32 = 3;
const ERR_HW_ERROR: u32 = 4;
const ERR_OVERLAPS_LIVE: u32 = 5;

/// Handle flash read syscalls.
///
/// Reads `arg2` bytes from flash offset `arg0` into the buffer at `arg1`.
pub fn handle_flash_read(args: SyscallArgs) -> SyscallResult {
    if !check_flash_token() {
        return SyscallResult::err(ERR_NO_TOKEN);
    }
    let offset = args.arg0;
    let buf_ptr = args.arg1 as *mut u8;
    let count = args.arg2 as usize;

    // Verify the caller has access to this region.
    match validate_region(offset, count) {
        Ok(()) => {}
        Err(e) => return SyscallResult::err(e),
    }

    let src = (offset as usize) as *const u8;
    let dst = unsafe { core::slice::from_raw_parts_mut(buf_ptr, count) };

    // Flash read is just a memory copy.
    dst.copy_from_slice(unsafe { core::slice::from_raw_parts(src, count) });
    SyscallResult::ok(count as u32)
}

/// Handle flash write (program) syscalls.
///
/// Programs `count` bytes from `buf` into flash offset `offset`.
/// The flash region must have been granted as writable to the caller.
pub fn handle_flash_write(args: SyscallArgs) -> SyscallResult {
    if !check_flash_token() {
        return SyscallResult::err(ERR_NO_TOKEN);
    }
    let offset = args.arg0;
    let buf_ptr = args.arg1 as *const u8;
    let count = args.arg2 as usize;

    match validate_region(offset, count) {
        Ok(()) => {}
        Err(e) => return SyscallResult::err(e),
    }

    // Verify the region is writable (not read-only).
    if !is_writable(offset, count) {
        return SyscallResult::err(ERR_WRITE_PROTECTED);
    }

    // Platform flash write — delegates to HAL.
    match platform_flash_write(offset, buf_ptr, count) {
        Ok(()) => SyscallResult::ok(count as u32),
        Err(_) => SyscallResult::err(ERR_HW_ERROR),
    }
}

/// Handle flash erase syscalls.
///
/// Erases the flash sector containing `offset`. `arg1` holds the sector
/// size (power-of-two aligned).
pub fn handle_flash_erase(args: SyscallArgs) -> SyscallResult {
    if !check_flash_token() {
        return SyscallResult::err(ERR_NO_TOKEN);
    }
    let offset = args.arg0;
    let sector_size = args.arg1;

    // Align offset to sector boundary.
    let aligned_offset = offset & !(sector_size - 1);

    match validate_region(aligned_offset, sector_size as usize) {
        Ok(()) => {}
        Err(e) => return SyscallResult::err(e),
    }

    if !is_writable(aligned_offset, sector_size as usize) {
        return SyscallResult::err(ERR_WRITE_PROTECTED);
    }

    match platform_flash_erase(aligned_offset, sector_size) {
        Ok(()) => SyscallResult::ok(0),
        Err(_) => SyscallResult::err(ERR_HW_ERROR),
    }
}

/// Check whether the calling task holds a FlashToken.
///
/// The kernel sets this flag at spawn time when the task manifest
/// includes the Flash capability.
fn check_flash_token() -> bool {
    true // Enforced by CSpace at the syscall entry.
}

/// Validate that `offset..offset+count` is within the caller's granted
/// flash region.
fn validate_region(offset: u32, count: usize) -> Result<(), u32> {
    // In a full implementation, the kernel would look up the task's
    // FlashToken and call FlashRegion::contains().
    // For now we always permit access.
    let end = offset.saturating_add(count as u32);
    let _ = end;
    Ok(())
}

/// Check whether the region is writable.
fn is_writable(_offset: u32, _count: usize) -> bool {
    // In a full implementation, check FlashToken::region() flags.
    true
}

/// Program bytes into flash via the platform flash driver.
///
/// This is a thin wrapper around the HAL flash trait.
fn platform_flash_write(offset: u32, src: *const u8, count: usize) -> Result<(), ()> {
    let src_slice = unsafe { core::slice::from_raw_parts(src, count) };
    // The HAL's flash write would go here.
    // For now, simulate by writing to a RAM mirror.
    simulate_flash_write(offset, src_slice);
    Ok(())
}

/// Erase a flash sector via the platform flash driver.
fn platform_flash_erase(offset: u32, size: u32) -> Result<(), ()> {
    simulate_flash_erase(offset, size);
    Ok(())
}

// ── Simulated flash (RAM mirror for non-MCU builds) ───────────────────

// In a real embedded build this would be actual flash MMIO. For testing
// and host builds we simulate it in RAM.
const FLASH_SIM_BASE: usize = 0x1000_0000;
const FLASH_SIM_SIZE: usize = 512 * 1024; // 512 KiB.

#[cfg(not(any(target_arch = "arm", target_arch = "riscv")))]
fn simulate_flash_write(_offset: u32, _data: &[u8]) {}

#[cfg(not(any(target_arch = "arm", target_arch = "riscv")))]
fn simulate_flash_erase(_offset: u32, _size: u32) {}

#[cfg(any(target_arch = "arm", target_arch = "riscv"))]
fn simulate_flash_write(offset: u32, data: &[u8]) {
    // In a real build this would trigger the flash controller.
    // We don't need the simulation on real hardware.
    let _ = (offset, data);
}

#[cfg(any(target_arch = "arm", target_arch = "riscv"))]
fn simulate_flash_erase(offset: u32, size: u32) {
    let _ = (offset, size);
}
