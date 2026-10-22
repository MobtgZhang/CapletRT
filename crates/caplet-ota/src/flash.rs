//! Minimal NOR-flash abstraction used by the boot manager and updater.
//!
//! The trait deliberately models the operations real flash controllers
//! expose: unlock, sector erase, program and lock.  It does *not* pretend a
//! plain memory write is a flash update.  [`RamFlash`] is a faithful host
//! implementation for tests and QEMU board shims.

/// Error returned by [`FlashStorage`] implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashIoError {
    /// Address range is outside the device.
    OutOfRange,
    /// Address or length is not erase-sector aligned.
    Unaligned,
    /// The controller is locked.
    Locked,
    /// Programming attempted to turn a zero bit back into one.
    NotErased,
    /// Injected power loss (used by tests).
    PowerCut,
    /// Backend-specific I/O failure.
    Io,
}

/// Operations required from a NOR flash driver.
///
/// All addresses are absolute physical flash addresses.  Implementations
/// must reject out-of-range and misaligned erase requests rather than
/// silently clamping them.
pub trait FlashStorage {
    /// Read exactly `dst.len()` bytes.
    fn read(&mut self, addr: u32, dst: &mut [u8]) -> Result<(), FlashIoError>;
    /// Erase a sector-aligned range.
    fn erase(&mut self, addr: u32, len: u32) -> Result<(), FlashIoError>;
    /// Program bytes.  Programming may only clear bits.
    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashIoError>;
    /// Leave write-protect state.
    fn unlock(&mut self) -> Result<(), FlashIoError>;
    /// Enter write-protect state.
    fn lock(&mut self) -> Result<(), FlashIoError>;
}

impl<T: FlashStorage + ?Sized> FlashStorage for &mut T {
    fn read(&mut self, addr: u32, dst: &mut [u8]) -> Result<(), FlashIoError> {
        (**self).read(addr, dst)
    }

    fn erase(&mut self, addr: u32, len: u32) -> Result<(), FlashIoError> {
        (**self).erase(addr, len)
    }

    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashIoError> {
        (**self).program(addr, data)
    }

    fn unlock(&mut self) -> Result<(), FlashIoError> {
        (**self).unlock()
    }

    fn lock(&mut self) -> Result<(), FlashIoError> {
        (**self).lock()
    }
}

/// A deterministic RAM-backed NOR flash.
///
/// The buffer starts erased (`0xFF`) and enforces the same 1 -> 0 programming
/// rule as NOR flash.  Tests can inject a power cut before the next mutating
/// operation and can corrupt one byte to model media damage.
pub struct RamFlash<const N: usize> {
    base: u32,
    sector_size: u32,
    bytes: [u8; N],
    unlocked: bool,
    power_cut_after_ops: Option<u32>,
}

impl<const N: usize> RamFlash<N> {
    /// Construct an erased device.
    pub fn new(base: u32, sector_size: u32) -> Self {
        assert!(sector_size > 0, "flash sector size must be non-zero");
        assert!(
            N as u32 >= sector_size,
            "flash must contain one full sector"
        );
        Self {
            base,
            sector_size,
            bytes: [0xFF; N],
            unlocked: false,
            power_cut_after_ops: None,
        }
    }

    /// Base physical address.
    pub const fn base(&self) -> u32 {
        self.base
    }

    /// Total size.
    pub const fn len(&self) -> u32 {
        N as u32
    }

    /// Whether the backing storage has zero capacity.
    pub const fn is_empty(&self) -> bool {
        N == 0
    }

    /// Whether the device is unlocked.
    pub const fn is_unlocked(&self) -> bool {
        self.unlocked
    }

    /// Access the backing bytes (host-side diagnostics/tests).
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    /// Mutably access the backing bytes (for fault injection only).
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    /// Fail the next `count` mutating operations with [`FlashIoError::PowerCut`].
    pub fn inject_power_cut_after(&mut self, count: u32) {
        self.power_cut_after_ops = Some(count);
    }

    /// Clear injected power-loss state.
    pub fn clear_power_cut(&mut self) {
        self.power_cut_after_ops = None;
    }

    fn offset(&self, addr: u32, len: usize) -> Result<usize, FlashIoError> {
        let start = addr
            .checked_sub(self.base)
            .ok_or(FlashIoError::OutOfRange)? as usize;
        let end = start.checked_add(len).ok_or(FlashIoError::OutOfRange)?;
        if end > N {
            return Err(FlashIoError::OutOfRange);
        }
        Ok(start)
    }

    fn before_mutation(&mut self) -> Result<(), FlashIoError> {
        if !self.unlocked {
            return Err(FlashIoError::Locked);
        }
        if let Some(remaining) = self.power_cut_after_ops {
            if remaining == 0 {
                self.power_cut_after_ops = None;
                return Err(FlashIoError::PowerCut);
            }
            self.power_cut_after_ops = Some(remaining - 1);
        }
        Ok(())
    }
}

impl<const N: usize> FlashStorage for RamFlash<N> {
    fn read(&mut self, addr: u32, dst: &mut [u8]) -> Result<(), FlashIoError> {
        let start = self.offset(addr, dst.len())?;
        dst.copy_from_slice(&self.bytes[start..start + dst.len()]);
        Ok(())
    }

    fn erase(&mut self, addr: u32, len: u32) -> Result<(), FlashIoError> {
        if self.sector_size == 0
            || addr % self.sector_size != 0
            || len == 0
            || len % self.sector_size != 0
        {
            return Err(FlashIoError::Unaligned);
        }
        let start = self.offset(addr, len as usize)?;
        self.before_mutation()?;
        self.bytes[start..start + len as usize].fill(0xFF);
        Ok(())
    }

    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashIoError> {
        let start = self.offset(addr, data.len())?;
        self.before_mutation()?;
        for (old, new) in self.bytes[start..start + data.len()].iter().zip(data) {
            if (*old & *new) != *new {
                return Err(FlashIoError::NotErased);
            }
        }
        for (old, new) in self.bytes[start..start + data.len()].iter_mut().zip(data) {
            *old &= *new;
        }
        Ok(())
    }

    fn unlock(&mut self) -> Result<(), FlashIoError> {
        self.unlocked = true;
        Ok(())
    }

    fn lock(&mut self) -> Result<(), FlashIoError> {
        self.unlocked = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erased_by_default_and_requires_unlock() {
        let mut flash = RamFlash::<8192>::new(0x1000_0000, 4096);
        let mut byte = [0u8; 1];
        flash.read(0x1000_0000, &mut byte).unwrap();
        assert_eq!(byte, [0xFF]);
        assert_eq!(flash.program(0x1000_0000, &[0]), Err(FlashIoError::Locked));
        flash.unlock().unwrap();
        flash.erase(0x1000_0000, 4096).unwrap();
        flash.program(0x1000_0000, &[0x5A]).unwrap();
        flash.read(0x1000_0000, &mut byte).unwrap();
        assert_eq!(byte, [0x5A]);
    }

    #[test]
    fn rejects_unaligned_erase_and_reprogramming_without_erase() {
        let mut flash = RamFlash::<8192>::new(0, 4096);
        flash.unlock().unwrap();
        assert_eq!(flash.erase(1, 4096), Err(FlashIoError::Unaligned));
        flash.program(4, &[0x7F]).unwrap();
        assert_eq!(flash.program(4, &[0xFF]), Err(FlashIoError::NotErased));
        flash.erase(0, 4096).unwrap();
        flash.program(4, &[0xFF]).unwrap();
    }

    #[test]
    fn power_cut_leaves_previous_contents_intact() {
        let mut flash = RamFlash::<8192>::new(0, 4096);
        flash.unlock().unwrap();
        flash.program(0, &[0x42]).unwrap();
        flash.inject_power_cut_after(0);
        assert_eq!(flash.erase(0, 4096), Err(FlashIoError::PowerCut));
        let mut byte = [0u8; 1];
        flash.read(0, &mut byte).unwrap();
        assert_eq!(byte, [0x42]);
    }
}
