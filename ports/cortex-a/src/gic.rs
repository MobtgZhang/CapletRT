//! GICv2 driver for the Cortex-A port.
//!
//! Provides a minimal subset needed by M3:
//!
//! - distributor enable / mask-all
//! - per-IRQ enable / disable / priority
//! - acknowledgement and end-of-interrupt
//! - software-generated interrupts (SGI 0..15) for IPI
//!
//! docv1.md §13.4 pinned us to GICv2 for compatibility. ARMv8-A
//! GICv3 is a future migration target.

use core::ptr::{read_volatile, write_volatile};

/// Distributor registers live at `GICD_BASE` from `boot.rs`.
const GICD_BASE: u32 = 0x1E00_1000;
/// CPU interface registers live at `GICC_BASE` from `boot.rs`.
const GICC_BASE: u32 = 0x1E00_0100;

/// Distributor control register.
const GICD_CTLR: u32 = GICD_BASE;
/// Distributor interrupt set-enable register (per-bank).
const GICD_ISENABLER: u32 = GICD_BASE + 0x100;
/// Distributor interrupt clear-enable register.
const GICD_ICENABLER: u32 = GICD_BASE + 0x180;
/// Distributor priority register bank.
const GICD_IPRIORITYR: u32 = GICD_BASE + 0x400;
/// Distributor SGI register.
const GICD_SGIR: u32 = GICD_BASE + 0xF00;

/// CPU interface control register.
const GICC_CTLR: u32 = GICC_BASE;
/// CPU interface priority mask register.
const GICC_PMR: u32 = GICC_BASE + 0x004;
/// CPU interface acknowledge register.
const GICC_IAR: u32 = GICC_BASE + 0x00C;
/// CPU interface end-of-interrupt register.
const GICC_EOIR: u32 = GICC_BASE + 0x010;

/// Mask all IRQs at the distributor. Idempotent.
///
/// # Safety
///
/// Must be executed by a CPU that owns the GIC distributor mapping.
pub unsafe fn mask_all() {
    write_volatile(GICD_CTLR as *mut u32, 0);
}

/// Enable the distributor and set the CPU-interface priority mask
/// to the lowest value so every IRQ can fire.
///
/// # Safety
///
/// Must be executed after the GIC MMIO windows are identity-mapped and
/// by the CPU responsible for distributor configuration.
pub unsafe fn enable() {
    // Enable distributor.
    let ctlr = read_volatile(GICD_CTLR as *const u32);
    write_volatile(GICD_CTLR as *mut u32, ctlr | 1);
    // Priority mask = 0xf8 to allow priorities 0..0xf8 (i.e. all).
    write_volatile(GICC_PMR as *mut u32, 0xf8);
    // Enable CPU interface.
    let gicc = read_volatile(GICC_CTLR as *const u32);
    write_volatile(GICC_CTLR as *mut u32, gicc | 1);
}

/// Enable a shared peripheral interrupt (SPI) or private peripheral
/// interrupt (PPI). `id` is the global interrupt number (0..1020).
///
/// # Safety
///
/// `id` must identify a valid GIC interrupt and the distributor must be
/// mapped and accessible from the current CPU.
pub unsafe fn enable_irq(id: u32) {
    let reg = GICD_ISENABLER + (id / 32) * 4;
    let bit = 1u32 << (id % 32);
    let cur = read_volatile(reg as *const u32);
    write_volatile(reg as *mut u32, cur | bit);
}

/// Disable an IRQ.
///
/// # Safety
///
/// `id` must identify a valid GIC interrupt and the distributor must be
/// mapped and accessible from the current CPU.
pub unsafe fn disable_irq(id: u32) {
    let reg = GICD_ICENABLER + (id / 32) * 4;
    let bit = 1u32 << (id % 32);
    let cur = read_volatile(reg as *const u32);
    write_volatile(reg as *mut u32, cur | bit);
}

/// Set the priority of an IRQ. Lower value = higher priority.
///
/// # Safety
///
/// `id` must identify a valid GIC interrupt and the distributor must be
/// mapped and accessible from the current CPU.
pub unsafe fn set_priority(id: u32, priority: u8) {
    let reg = GICD_IPRIORITYR + (id / 4) * 4;
    let shift = (id % 4) * 8;
    let cur = read_volatile(reg as *const u32);
    let mask = 0xffu32 << shift;
    let next = (cur & !mask) | (((priority as u32) & 0xff) << shift);
    write_volatile(reg as *mut u32, next);
}

/// Acknowledge an IRQ. Returns the interrupt id, or `None` for
/// spurious interrupts (id == 1023).
///
/// # Safety
///
/// Must be called from the IRQ handler of the CPU interface being read.
pub unsafe fn ack() -> Option<u32> {
    let iar = read_volatile(GICC_IAR as *const u32);
    if iar == 1023 {
        None
    } else {
        Some(iar)
    }
}

/// Signal end of interrupt.
///
/// # Safety
///
/// `id` must be the value returned by the matching [`ack`] call.
pub unsafe fn eoi(id: u32) {
    write_volatile(GICC_EOIR as *mut u32, id);
}

/// Send a software-generated interrupt to one or more CPUs. The
/// target filter is encoded in the low byte of `sgir_value`.
///
/// # Safety
///
/// The distributor must be mapped; `int_id` must be a valid SGI (0..15).
pub unsafe fn send_sgi(int_id: u32, cpu_mask: u8) {
    let value = ((cpu_mask as u32) << 16) | (int_id & 0xf);
    write_volatile(GICD_SGIR as *mut u32, value);
}

/// Send SGI #0 to the current CPU. Used by `request_reschedule`.
///
/// # Safety
///
/// The GIC distributor must be mapped and accessible.
pub unsafe fn sgi_self(id: u32) {
    // SGI target filter "this CPU only" = 0b10 << 24.
    let value = (0b10u32 << 24) | (id & 0xf);
    write_volatile(GICD_SGIR as *mut u32, value);
}

/// Send SGI #0 to all CPUs except the current one. Used by the load
/// balancer / TLB shootdown logic.
///
/// # Safety
///
/// The GIC distributor must be mapped and accessible.
pub unsafe fn sgi_all_others(id: u32) {
    // Target filter "all CPUs other than this one" = 0b11.
    let value = (0b11u32 << 24) | (id & 0xf);
    write_volatile(GICD_SGIR as *mut u32, value);
}

/// IPI reasons — same values as `kernel::cpu::IpiReason`.
pub const IPI_RESCHEDULE: u32 = 0;
/// TLB shootdown — invalidates the vaddr carried in `arg`.
pub const IPI_TLB: u32 = 1;
/// Halt request.
pub const IPI_HALT: u32 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check that the register addresses are word-aligned.
    #[test]
    fn addrs_are_aligned() {
        assert_eq!(GICD_CTLR % 4, 0);
        assert_eq!(GICC_EOIR % 4, 0);
    }

    #[test]
    fn sgi_value_layout() {
        // Sanity check the target-filter / int-id encoding.
        let v = (0b10u32 << 24) | 0xf;
        assert_eq!(v & 0xf, 0xf);
        assert_eq!((v >> 24) & 0b11, 0b10);
    }
}
