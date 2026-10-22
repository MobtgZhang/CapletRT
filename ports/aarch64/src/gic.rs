//! ARM Generic Interrupt Controller support for GICv2 and GICv3.
//!
//! QEMU's `virt` machine exposes a GICv2 (distributor at
//! `0x0800_0000`, CPU interface at `0x0801_0000`) while the Renode
//! Cortex-A53 platform describes a GICv3 with a redistributor at
//! `0x080A_0000`.  The port detects the revision at runtime and drives
//! the corresponding interface.

/// GIC distributor base address on both simulators.
pub const GICD_BASE: usize = 0x0800_0000;
/// GICv2 CPU interface base address (QEMU virt).
pub const GICC_BASE: usize = 0x0801_0000;
/// GICv3 redistributor base address (Renode Cortex-A53).
pub const GICR_BASE: usize = 0x080A_0000;
/// Generic timer physical EL1 PPI.
pub const TIMER_IRQ: u32 = 30;
/// Software-generated interrupt used for reschedule IPIs.
pub const IPI_RESCHEDULE_IRQ: u32 = 0;

const _: () = assert!(TIMER_IRQ < 32);

const GICD_CTLR: usize = 0x0000;
const GICD_ISENABLER: usize = 0x0100;
const GICD_ICENABLER: usize = 0x0180;
const GICD_IPRIORITYR: usize = 0x0400;
const GICD_SGIR: usize = 0x0f00;
/// Architecture revision field (bits 7:4) of the distributor's
/// peripheral ID register 2.
const GICD_PIDR2: usize = 0x0fe8;

const GICC_CTLR: usize = 0x0000;
const GICC_PMR: usize = 0x0004;
const GICC_IAR: usize = 0x000c;
const GICC_EOIR: usize = 0x0010;

const GICR_WAKER: usize = 0x0014;
const GICR_TYPER: usize = 0x0008;
const GICR_IGROUPR0: usize = 0x0080;
const GICR_ISENABLER0: usize = 0x0100;
const GICR_ICENABLER0: usize = 0x0180;
const GICR_IPRIORITYR0: usize = 0x0400;

const WAKER_PROCESSOR_SLEEP: u32 = 1 << 1;
const WAKER_CHILDREN_ASLEEP: u32 = 1 << 2;

/// Interrupt-controller revision detected from `GICD_PIDR2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// GICv2 with a memory-mapped CPU interface.
    V2,
    /// GICv3 with system-register CPU interface and redistributors.
    V3,
    /// Unknown or unsupported revision.
    Unknown(u8),
}

fn read32(address: usize) -> u32 {
    // SAFETY: callers only pass fixed MMIO register addresses.
    unsafe { core::ptr::read_volatile(address as *const u32) }
}

fn write32(address: usize, value: u32) {
    // SAFETY: callers only pass fixed MMIO register addresses.
    unsafe { core::ptr::write_volatile(address as *mut u32, value) };
}

/// Detect the interrupt-controller architecture revision.
///
/// The distributor's `PIDR2` is always mapped on both GICv2 and GICv3
/// systems, so consult it first.  Some Renode releases leave that ID
/// register unimplemented (it reads as zero); only in that case may we
/// probe the redistributor.  Unconditionally touching `GICR_BASE` is
/// invalid on a GICv2 platform such as QEMU's default `virt` machine:
/// there is no redistributor there and the read raises a Data Abort.
pub fn version() -> Version {
    match ((read32(GICD_BASE + GICD_PIDR2) >> 4) & 0xf) as u8 {
        2 => return Version::V2,
        3 => return Version::V3,
        _ => {}
    }

    // Fallback for models without peripheral ID registers.  Reaching
    // this point means the distributor did not identify itself as v2,
    // so probing the redistributor cannot mis-detect a normal GICv2.
    if read32(GICR_BASE + GICR_TYPER) != 0 {
        Version::V3
    } else {
        let revision = ((read32(GICD_BASE + GICD_PIDR2) >> 4) & 0xf) as u8;
        Version::Unknown(revision)
    }
}

/// Initialise the detected interrupt controller and enable the timer
/// PPI.  Interrupt delivery at the CPU is intentionally left disabled
/// by [`crate::arch::AArch64Arch::early_init`]; this keeps the
/// cooperative scheduler usable before an exception vector is
/// installed while still allowing tests and future preemptive paths to
/// use the controller.
pub fn init() -> Version {
    let version = version();
    match version {
        Version::V2 => init_v2(),
        Version::V3 => init_v3(),
        Version::Unknown(_) => {}
    }
    version
}

fn init_v2() {
    write32(GICD_BASE + GICD_CTLR, 0);
    write32(GICC_BASE + GICC_CTLR, 0);
    write32(GICC_BASE + GICC_PMR, 0xff);
    set_priority_v2(TIMER_IRQ, 0xa0);
    enable_v2(TIMER_IRQ);
    write32(GICD_BASE + GICD_CTLR, 1);
    write32(GICC_BASE + GICC_CTLR, 1);
}

fn init_v3() {
    write32(GICD_BASE + GICD_CTLR, 0);

    // Wake the redistributor for CPU 0.
    let waker = GICR_BASE + GICR_WAKER;
    let mut value = read32(waker);
    value &= !WAKER_PROCESSOR_SLEEP;
    write32(waker, value);
    let mut spins = 1_000_000u32;
    while read32(waker) & WAKER_CHILDREN_ASLEEP != 0 && spins != 0 {
        spins -= 1;
        core::hint::spin_loop();
    }

    // Route PPIs (including the EL1 physical timer) to group 1, give the
    // timer a priority that is not masked by ICC_PMR_EL1, and enable it.
    // Renode 1.17 may ignore some of these configuration frames; the
    // writes are still required on hardware and on QEMU.
    write32(GICR_BASE + GICR_IGROUPR0, 0xffff_ffff);
    set_priority_v3(TIMER_IRQ, 0xa0);
    enable_v3(TIMER_IRQ);

    #[cfg(target_arch = "aarch64")]
    {
        let sre: u64;
        // SAFETY: ICC_SRE_EL1 is accessible at EL1 on a GICv3 system.
        unsafe { core::arch::asm!("mrs {0}, ICC_SRE_EL1", out(reg) sre, options(nomem, nostack)) };
        let sre = sre | 0x7;
        unsafe {
            core::arch::asm!(
                "msr ICC_SRE_EL1, {sre}",
                "isb",
                "msr ICC_PMR_EL1, {pmr}",
                "msr ICC_IGRPEN1_EL1, {enable}",
                "isb",
                sre = in(reg) sre,
                pmr = in(reg) 0xffu64,
                enable = in(reg) 1u64,
                options(nostack),
            );
        }
    }

    // Affinity routing and group 1 non-secure.
    write32(GICD_BASE + GICD_CTLR, (1 << 4) | (1 << 1));
}

/// Enable an interrupt by global interrupt id.
pub fn enable(id: u32) {
    match version() {
        Version::V2 => enable_v2(id),
        Version::V3 => enable_v3(id),
        Version::Unknown(_) => {}
    }
}

fn enable_v2(id: u32) {
    let offset = GICD_ISENABLER + (id as usize / 32) * 4;
    write32(GICD_BASE + offset, 1 << (id % 32));
}

fn enable_v3(id: u32) {
    if id < 32 {
        write32(GICR_BASE + GICR_ISENABLER0, 1 << id);
    } else {
        let offset = GICD_ISENABLER + (id as usize / 32) * 4;
        write32(GICD_BASE + offset, 1 << (id % 32));
    }
}

/// Disable an interrupt by global interrupt id.
pub fn disable(id: u32) {
    match version() {
        Version::V2 => {
            let offset = GICD_ICENABLER + (id as usize / 32) * 4;
            write32(GICD_BASE + offset, 1 << (id % 32));
        }
        Version::V3 if id < 32 => {
            write32(GICR_BASE + GICR_ICENABLER0, 1 << id);
        }
        Version::V3 => {
            let offset = GICD_ICENABLER + (id as usize / 32) * 4;
            write32(GICD_BASE + offset, 1 << (id % 32));
        }
        Version::Unknown(_) => {}
    }
}

/// Set the priority of an interrupt (lower value means higher priority).
pub fn set_priority(id: u32, priority: u8) {
    match version() {
        Version::V2 => set_priority_v2(id, priority),
        Version::V3 => set_priority_v3(id, priority),
        Version::Unknown(_) => {}
    }
}

fn set_priority_v2(id: u32, priority: u8) {
    let address = GICD_BASE + GICD_IPRIORITYR + (id as usize & !3);
    let shift = (id & 3) * 8;
    let mut value = read32(address);
    value = (value & !(0xff << shift)) | ((priority as u32) << shift);
    write32(address, value);
}

fn set_priority_v3(id: u32, priority: u8) {
    let (base, offset) = if id < 32 {
        (GICR_BASE, GICR_IPRIORITYR0)
    } else {
        (GICD_BASE, GICD_IPRIORITYR)
    };
    let address = base + offset + (id as usize & !3);
    let shift = (id & 3) * 8;
    let mut value = read32(address);
    value = (value & !(0xff << shift)) | ((priority as u32) << shift);
    write32(address, value);
}

/// Acknowledge an interrupt. Returns `None` for a spurious interrupt.
pub fn ack() -> Option<u32> {
    let raw = match version() {
        Version::V2 => read32(GICC_BASE + GICC_IAR),
        Version::V3 => {
            #[cfg(target_arch = "aarch64")]
            {
                let value: u64;
                // SAFETY: ICC_IAR1_EL1 is accessible after GICv3 init.
                unsafe {
                    core::arch::asm!("mrs {0}, ICC_IAR1_EL1", out(reg) value, options(nomem, nostack))
                };
                value as u32
            }
            #[cfg(not(target_arch = "aarch64"))]
            {
                1023
            }
        }
        Version::Unknown(_) => 1023,
    };
    if raw >= 1020 {
        None
    } else {
        Some(raw)
    }
}

/// Signal end of interrupt.
pub fn eoi(id: u32) {
    match version() {
        Version::V2 => write32(GICC_BASE + GICC_EOIR, id),
        Version::V3 => {
            #[cfg(target_arch = "aarch64")]
            // SAFETY: ICC_EOIR1_EL1 requires the acknowledged id.
            unsafe {
                core::arch::asm!(
                    "msr ICC_EOIR1_EL1, {id}",
                    id = in(reg) id as u64,
                    options(nostack),
                );
            }
        }
        Version::Unknown(_) => {}
    }
}

/// Send an SGI to every CPU in `target_list` (Aff0 mask on GICv3).
pub fn send_sgi(id: u32, target_list: u8) {
    match version() {
        Version::V2 => {
            // Target-list filter (0b00) plus the 8-bit CPU target mask.
            write32(
                GICD_BASE + GICD_SGIR,
                ((target_list as u32) << 16) | (id & 0xf),
            );
        }
        Version::V3 => {
            #[cfg(target_arch = "aarch64")]
            {
                let value = (target_list as u64) | (((id & 0xf) as u64) << 24);
                // SAFETY: ICC_SGI1R_EL1 sends an SGI in the current
                // security state; affinity fields are zero for CPU 0.
                unsafe {
                    core::arch::asm!(
                        "msr ICC_SGI1R_EL1, {value}",
                        value = in(reg) value,
                        options(nostack),
                    );
                }
            }
        }
        Version::Unknown(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_offsets_are_word_aligned() {
        for offset in [
            GICD_CTLR,
            GICD_ISENABLER,
            GICD_ICENABLER,
            GICD_IPRIORITYR,
            GICD_SGIR,
            GICC_CTLR,
            GICC_PMR,
            GICC_IAR,
            GICC_EOIR,
            GICR_WAKER,
            GICR_TYPER,
            GICR_IGROUPR0,
            GICR_ISENABLER0,
            GICR_ICENABLER0,
            GICR_IPRIORITYR0,
        ] {
            assert_eq!(offset % 4, 0, "offset {offset:#x}");
        }
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn timer_ppi_is_private() {
        assert!(TIMER_IRQ < 32);
    }
}
