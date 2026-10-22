//! Flash syscall handler.
//!
//! Flash access is capability-gated.  The caller must hold the named
//! `flash:0` capability whose [`FlashGrant`](crate::security_grants::FlashGrant)
//! describes the permitted byte range.  Read access requires `READ`; program
//! and erase additionally require `WRITE` and a writable grant.
//!
//! Syscall protocol:
//!
//! | syscall | arg0 | arg1 | arg2 | arg3 |
//! |---------|------|------|------|------|
//! | `FlashRead` | flash address | destination buffer | byte count | - |
//! | `FlashWrite` | flash address | source buffer | byte count | - |
//! | `FlashErase` | sector address | sector size (or 0) | - | - |
//!
//! The kernel keeps a RAM-backed mirror of the simulated NOR flash.  This is
//! used both by host tests and by Renode/QEMU board images that do not expose
//! a memory-mapped flash controller.  Programming enforces the physical NOR
//! rule that bits may only change from 1 to 0; erasing sets a whole sector to
//! `0xFF`.

use crate::security_grants::{current_flash_grant, FlashGrant};
use crate::syscall::{SyscallArgs, SyscallResult, WideSyscallArgs};
use alloc::vec;
use alloc::vec::Vec;
use caplet_cap::permissions::perm;
use caplet_cap::Permissions;
use caplet_sync::SpinLock;

/// Caller does not hold a flash capability.
pub const ERR_NO_TOKEN: u32 = 1;
/// Access falls outside the granted/backing flash range.
pub const ERR_OUT_OF_REGION: u32 = 2;
/// The grant is read-only.
pub const ERR_WRITE_PROTECTED: u32 = 3;
/// The backing store reported a failure.
pub const ERR_HW_ERROR: u32 = 4;
/// The operation would modify live code (reserved for board policy).
pub const ERR_OVERLAPS_LIVE: u32 = 5;
/// Programming attempted a 0 -> 1 transition without erasing.
pub const ERR_NOT_ERASED: u32 = 6;
/// Erase address/size is not sector aligned.
pub const ERR_BAD_ALIGNMENT: u32 = 7;
/// A buffer pointer is null.
pub const ERR_BAD_BUFFER: u32 = 8;

/// Base address of the simulated flash window.
pub const FLASH_BASE: u32 = 0x1000_0000;
/// Size of the simulated flash window (512 KiB).
pub const FLASH_SIZE: usize = 512 * 1024;
/// Erase sector size of the simulated flash.
pub const FLASH_SECTOR_SIZE: u32 = 4096;

static FLASH_MIRROR: SpinLock<Option<Vec<u8>>> = SpinLock::new(None);

fn with_mirror<R>(f: impl FnOnce(&mut [u8]) -> R) -> R {
    let mut guard = FLASH_MIRROR.lock();
    if guard.is_none() {
        *guard = Some(vec![0xFFu8; FLASH_SIZE]);
    }
    f(guard.as_mut().expect("initialised above"))
}

/// Erase the whole simulated flash (all bytes become `0xFF`).
pub fn reset_flash_mirror() {
    with_mirror(|flash| flash.fill(0xFF));
}

/// Overwrite part of the mirror without NOR programming rules.
///
/// This models an external programmer/factory image and is intended for
/// tests and board bring-up only.  It does not bypass syscall capabilities.
pub fn seed_flash_mirror(offset: u32, data: &[u8]) -> Result<(), u32> {
    let start = offset.checked_sub(FLASH_BASE).ok_or(ERR_OUT_OF_REGION)? as usize;
    let end = start.checked_add(data.len()).ok_or(ERR_OUT_OF_REGION)?;
    if end > FLASH_SIZE {
        return Err(ERR_OUT_OF_REGION);
    }
    with_mirror(|flash| flash[start..end].copy_from_slice(data));
    Ok(())
}

/// Copy bytes out of the simulated flash without capability checks.
///
/// Like [`seed_flash_mirror`], this is a bring-up/test helper; tasks must use
/// the syscall interface.
pub fn peek_flash_mirror(offset: u32, out: &mut [u8]) -> Result<(), u32> {
    let start = offset.checked_sub(FLASH_BASE).ok_or(ERR_OUT_OF_REGION)? as usize;
    let end = start.checked_add(out.len()).ok_or(ERR_OUT_OF_REGION)?;
    if end > FLASH_SIZE {
        return Err(ERR_OUT_OF_REGION);
    }
    with_mirror(|flash| out.copy_from_slice(&flash[start..end]));
    Ok(())
}

/// Handle `FlashRead` with the native 32-bit argument ABI.
pub fn handle_flash_read(args: SyscallArgs) -> SyscallResult {
    read_impl(args.arg0, args.arg1 as usize, args.arg2 as usize)
}

/// Handle `FlashRead` with full-width buffer pointers.
pub fn handle_flash_read_wide(args: WideSyscallArgs) -> SyscallResult {
    read_impl(
        args.arg0,
        args.arg1_ptr::<u8>() as usize,
        args.arg2_lo as usize,
    )
}

/// Handle `FlashWrite` with the native 32-bit argument ABI.
pub fn handle_flash_write(args: SyscallArgs) -> SyscallResult {
    write_impl(args.arg0, args.arg1 as usize, args.arg2 as usize)
}

/// Handle `FlashWrite` with full-width buffer pointers.
pub fn handle_flash_write_wide(args: WideSyscallArgs) -> SyscallResult {
    write_impl(
        args.arg0,
        args.arg1_ptr::<u8>() as usize,
        args.arg2_lo as usize,
    )
}

/// Handle `FlashErase` with the native 32-bit argument ABI.
pub fn handle_flash_erase(args: SyscallArgs) -> SyscallResult {
    erase_impl(args.arg0, args.arg1)
}

/// Handle `FlashErase` with the wide ABI (sector size in `arg1_lo`).
pub fn handle_flash_erase_wide(args: WideSyscallArgs) -> SyscallResult {
    erase_impl(args.arg0, args.arg1_lo)
}

fn grant_and_perms() -> Result<(FlashGrant, Permissions), SyscallResult> {
    current_flash_grant().ok_or_else(|| SyscallResult::err(ERR_NO_TOKEN))
}

fn range_in_mirror(start: u32, count: usize) -> Result<(usize, usize), SyscallResult> {
    if count == 0 {
        let index = start
            .checked_sub(FLASH_BASE)
            .ok_or_else(|| SyscallResult::err(ERR_OUT_OF_REGION))? as usize;
        if index > FLASH_SIZE {
            return Err(SyscallResult::err(ERR_OUT_OF_REGION));
        }
        return Ok((index, index));
    }
    let end_addr = start
        .checked_add(count as u32 - 1)
        .ok_or_else(|| SyscallResult::err(ERR_OUT_OF_REGION))?;
    let first = start
        .checked_sub(FLASH_BASE)
        .ok_or_else(|| SyscallResult::err(ERR_OUT_OF_REGION))? as usize;
    let last = end_addr
        .checked_sub(FLASH_BASE)
        .ok_or_else(|| SyscallResult::err(ERR_OUT_OF_REGION))? as usize;
    if last >= FLASH_SIZE {
        return Err(SyscallResult::err(ERR_OUT_OF_REGION));
    }
    Ok((first, last + 1))
}

fn validate_region(grant: &FlashGrant, start: u32, count: usize) -> Result<(), SyscallResult> {
    let end = if count == 0 {
        start
    } else {
        start
            .checked_add(count as u32 - 1)
            .ok_or_else(|| SyscallResult::err(ERR_OUT_OF_REGION))?
    };
    if !grant.contains(start, end) {
        return Err(SyscallResult::err(ERR_OUT_OF_REGION));
    }
    Ok(())
}

fn read_impl(offset: u32, dst: usize, count: usize) -> SyscallResult {
    let (grant, _perms) = match grant_and_perms() {
        Ok(v) => v,
        Err(e) => return e,
    };
    if dst == 0 && count != 0 {
        return SyscallResult::err(ERR_BAD_BUFFER);
    }
    if let Err(e) = validate_region(&grant, offset, count) {
        return e;
    }
    let (start, end) = match range_in_mirror(offset, count) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let dst = dst as *mut u8;
    with_mirror(|flash| unsafe {
        core::ptr::copy_nonoverlapping(flash[start..end].as_ptr(), dst, count);
    });
    SyscallResult::ok(count as u32)
}

fn write_impl(offset: u32, src: usize, count: usize) -> SyscallResult {
    let (grant, perms) = match grant_and_perms() {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !perms.contains(perm::WRITE) || !grant.writable {
        return SyscallResult::err(ERR_WRITE_PROTECTED);
    }
    if src == 0 && count != 0 {
        return SyscallResult::err(ERR_BAD_BUFFER);
    }
    if let Err(e) = validate_region(&grant, offset, count) {
        return e;
    }
    let (start, end) = match range_in_mirror(offset, count) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if count == 0 {
        return SyscallResult::ok(0);
    }
    let src = src as *const u8;

    with_mirror(|flash| {
        // Validate the complete update before changing anything so a failed
        // program cannot leave a partially programmed sector.
        for (offset, slot) in flash[start..end].iter().enumerate() {
            let value = unsafe { core::ptr::read(src.add(offset)) };
            if value & !*slot != 0 {
                return SyscallResult::err(ERR_NOT_ERASED);
            }
        }
        for (offset, slot) in flash[start..end].iter_mut().enumerate() {
            let value = unsafe { core::ptr::read(src.add(offset)) };
            *slot &= value;
        }
        SyscallResult::ok(count as u32)
    })
}

fn erase_impl(offset: u32, requested_sector_size: u32) -> SyscallResult {
    let (grant, perms) = match grant_and_perms() {
        Ok(v) => v,
        Err(e) => return e,
    };
    if !perms.contains(perm::WRITE) || !grant.writable {
        return SyscallResult::err(ERR_WRITE_PROTECTED);
    }
    let sector_size = if requested_sector_size == 0 {
        grant.sector_size
    } else {
        requested_sector_size
    };
    if sector_size != grant.sector_size {
        return SyscallResult::err(ERR_BAD_ALIGNMENT);
    }
    if offset % sector_size != 0 {
        return SyscallResult::err(ERR_BAD_ALIGNMENT);
    }
    if let Err(e) = validate_region(&grant, offset, sector_size as usize) {
        return e;
    }
    let (start, end) = match range_in_mirror(offset, sector_size as usize) {
        Ok(v) => v,
        Err(e) => return e,
    };
    with_mirror(|flash| flash[start..end].fill(0xFF));
    SyscallResult::ok(0)
}
