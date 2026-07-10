//! CLINT (Core-Local Interruptor) driver for QEMU virt.
//!
//! The CLINT provides:
//!   - msip: machine software interrupt (per-hart)
//!   - mtime: 64-bit global timer
//!   - mtimecmp: per-hart timer compare register
//!
//! QEMU virt address map:
//!   CLINT_BASE + 0x0000  msip for hart 0  (4B, R/W)
//!   CLINT_BASE + 0x4000  mtime  (64B, R/W)
//!   CLINT_BASE + 0x8000  mtimecmp for hart 0 (64B, R/W)
//!
//! This driver is shared between RV32 and RV64; the mtime/mtimecmp
//! registers are always 64-bit (accessed via two 32-bit reads on RV32).

/// CLINT MMIO base on QEMU virt.
const CLINT_BASE: usize = 0x0200_0000;
/// Offset to msip for hart 0.
const MSIP_OFFSET: usize = 0x0;
/// Offset to mtime (64-bit).
const MTIME_OFFSET: usize = 0xBFF8;
/// Offset to mtimecmp for hart 0.
const MTIMECMP_OFFSET: usize = 0x4000;

/// The CLINT device — a singleton on QEMU virt.
pub struct Clint;

/// MMIO register access helpers.
///
/// On RV32, mtime/mtimecmp are 64-bit but QEMU maps them as two 32-bit
/// words at consecutive offsets. We use volatile reads/writes.
impl Clint {
    /// Initialize the CLINT (zero timer, clear software interrupts).
    pub fn init() {
        // mtime = 0
        Self::write_mtime(0);
        // mtimecmp = u64::MAX (never fires initially) for hart 0
        Self::write_mtimecmp(0, u64::MAX);
        // Clear msip
        Self::clear_software_irq(0);
    }

    /// Read the 64-bit mtime register.
    pub fn mtime() -> u64 {
        let lo = Self::read32(MTIME_OFFSET);
        let hi = Self::read32(MTIME_OFFSET + 4);
        ((hi as u64) << 32) | (lo as u64)
    }

    /// Write the 64-bit mtime register.
    pub fn write_mtime(val: u64) {
        Self::write32(MTIME_OFFSET, val as u32);
        Self::write32(MTIME_OFFSET + 4, (val >> 32) as u32);
    }

    /// Write the 64-bit mtimecmp register for `hart`.
    ///
    /// When mtime >= mtimecmp, a timer interrupt fires (STI).
    /// Writing u64::MAX disables the timer interrupt.
    pub fn write_mtimecmp(hart: u32, val: u64) {
        // QEMU virt maps mtimecmp as a flat array indexed by hart id.
        let offset = MTIMECMP_OFFSET + (hart as usize) * 8;
        Self::write32(offset, val as u32);
        Self::write32(offset + 4, (val >> 32) as u32);
    }

    /// Set the next timer deadline.
    ///
    /// Equivalent to: mtimecmp = mtime + delta_ticks
    pub fn set_deadline(delta_ticks: u64) {
        let now = Self::mtime();
        Self::write_mtimecmp(0, now.saturating_add(delta_ticks));
    }

    /// Acknowledge a timer interrupt by advancing mtimecmp past mtime.
    ///
    /// This is the standard RISC-V pattern: clear the pending timer
    /// interrupt by making the condition mtime >= mtimecmp false again.
    pub fn acknowledge_timer_irq() {
        // Set mtimecmp = mtime + 1 (timer fires immediately when we
        // next set a real deadline).
        let now = Self::mtime();
        Self::write_mtimecmp(0, now + 1);
        // Then set it to u64::MAX to fully disable until next set_deadline.
        Self::write_mtimecmp(0, u64::MAX);
    }

    /// Send a software interrupt (msip) to `hart`.
    ///
    /// This is how the scheduler triggers a context switch from another
    /// hart or from an interrupt handler.
    pub fn send_software_irq(hart: u32) {
        let offset = MSIP_OFFSET + (hart as usize) * 4;
        Self::write32(offset, 1);
    }

    /// Clear the software interrupt for `hart`.
    pub fn clear_software_irq(hart: u32) {
        let offset = MSIP_OFFSET + (hart as usize) * 4;
        Self::write32(offset, 0);
    }

    /// Read a 32-bit MMIO register.
    fn read32(offset: usize) -> u32 {
        let raw = (CLINT_BASE + offset) as *mut u32;
        // SAFETY: the address is a known MMIO register in the CLINT range.
        unsafe { core::ptr::read_volatile(raw) }
    }

    /// Write a 32-bit MMIO register.
    fn write32(offset: usize, val: u32) {
        let raw = (CLINT_BASE + offset) as *mut u32;
        // SAFETY: the address is a known MMIO register in the CLINT range.
        unsafe { core::ptr::write_volatile(raw, val) };
    }
}
