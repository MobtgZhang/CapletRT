//! PLIC (Platform-Level Interrupt Controller) driver for QEMU virt.
//!
//! The PLIC on QEMU virt connects external interrupt sources to hart 0.
//! Priority and enable bits are per-context (hart 0 S-mode = context 2).
//!
//! QEMU virt interrupt sources:
//!   IRQ 0   : none (no interrupt)
//!   IRQ 1-9 : reserved / platform
//!   IRQ 10  : UART0 (ns16550a)
//!   IRQ 11  : virtio-net
//!   IRQ ... : additional devices
//!
//! Address map (QEMU virt):
//!   PLIC_BASE + 0x000000  Priority register array   (num_sources × 4B)
//!   PLIC_BASE + 0x001000  Pending register array    (num_sources/32 × 4B)
//!   PLIC_BASE + 0x002000  Enable register array     (num_harts × 0x100 × 4B)
//!   PLIC_BASE + 0x200000  Priority threshold        (4B per hart)
//!   PLIC_BASE + 0x200004  Claim/complete register  (4B per hart)
//!
//! Priority 0 = never interrupt, 1-7 = increasing priority.
//! We configure IRQ 10 (UART0) with priority 3 for development.

/// PLIC MMIO base on QEMU virt.
const PLIC_BASE: usize = 0x0C00_0000;

/// Number of interrupt sources on QEMU virt (matches QEMU default).
const NUM_SOURCES: usize = 64;
/// Number of contexts (harts × modes) on QEMU virt.
const NUM_CONTEXTS: usize = 4;

/// PLIC interrupt context: 0 = M-mode, 1 = S-mode (unused), 2 = S-mode hart 0.
const CONTEXT_HART0_S: usize = 2;

/// The PLIC device.
pub struct Plic;

impl Plic {
    /// Initialize the PLIC — disable all external interrupts.
    pub fn init() {
        // Set priority of all sources to 0 (disabled).
        for i in 0..NUM_SOURCES {
            Self::write_priority(i as u32, 0);
        }
        // Disable all interrupts for our context (hart 0 S-mode).
        for i in 0..NUM_SOURCES {
            Self::write_enable(CONTEXT_HART0_S, i as u32, false);
        }
        // Set threshold to 0 (accept all non-zero priorities).
        Self::write_threshold(CONTEXT_HART0_S, 0);
    }

    /// Enable interrupt `irq` for hart 0 S-mode with the given priority.
    ///
    /// `irq` must be >= 1 and < NUM_SOURCES.
    /// `priority` must be 1..7 (0 = disabled).
    pub fn enable_irq(irq: u32, priority: u8) {
        Self::write_priority(irq, priority);
        Self::write_enable(CONTEXT_HART0_S, irq, true);
    }

    /// Acknowledge the highest-pending interrupt.
    ///
    /// Call this in the PLIC external interrupt handler.
    /// Returns the IRQ number of the interrupt being claimed (0 = none).
    pub fn acknowledge() -> u32 {
        let claim = Self::read32(Self::claim_addr(CONTEXT_HART0_S));
        claim
    }

    /// Mark interrupt `irq` as complete (end of interrupt handling).
    pub fn complete(irq: u32) {
        Self::write32(Self::claim_addr(CONTEXT_HART0_S), irq);
    }

    /// Handle the pending external interrupt.
    ///
    /// Called from the trap vector. Claims the interrupt from the PLIC,
    /// dispatches to the appropriate handler, then completes it.
    pub fn handle_pending() {
        let irq = Self::acknowledge();
        if irq == 0 {
            return;
        }
        // Dispatch based on IRQ number.
        match irq {
            #[cfg(feature = "uart")]
            10 => {
                // UART0 interrupt — read RHR, handle receive.
                crate::uart::Uart::handle_irq();
            }
            _ => {
                // Unknown or unhandled IRQ — just complete it.
                #[cfg(feature = "debug_irq")]
                {
                    crate::uart::Uart::put_char(b'?');
                    crate::uart::Uart::put_char(b'0' + ((irq / 10) % 10) as u8);
                    crate::uart::Uart::put_char(b'0' + (irq % 10) as u8);
                    crate::uart::Uart::put_char(b'\n');
                }
            }
        }
        Self::complete(irq);
    }

    /// Check if there is a pending interrupt for our context.
    pub fn has_pending() -> bool {
        // Scan the pending register.
        let pending_base = PLIC_BASE + 0x001000;
        let num_words = (NUM_SOURCES + 31) / 32;
        for i in 0..num_words {
            let val = Self::read32(pending_base + i * 4);
            if val != 0 {
                return true;
            }
        }
        false
    }

    // ── MMIO helpers ────────────────────────────────────────────────

    /// Write priority for interrupt `source`.
    fn write_priority(source: u32, priority: u8) {
        let offset = PLIC_BASE + 0x0 + (source as usize) * 4;
        // Priority is in bits [2:0]; mask to 7.
        Self::write32(offset, (priority & 7) as u32);
    }

    /// Write enable bit for `source` in the given `context`.
    fn write_enable(context: usize, source: u32, enabled: bool) {
        let enable_offset = 0x002000 + context * 0x100;
        let word = (source / 32) as usize;
        let bit = source % 32;
        let reg = enable_offset + word * 4;
        let current = Self::read32(reg);
        let new = if enabled {
            current | (1 << bit)
        } else {
            current & !(1 << bit)
        };
        Self::write32(reg, new);
    }

    /// Write the priority threshold for `context`.
    fn write_threshold(context: usize, threshold: u8) {
        let offset = PLIC_BASE + 0x200000 + context * 0x1000;
        Self::write32(offset, threshold as u32);
    }

    /// Address of the claim/complete register for `context`.
    fn claim_addr(context: usize) -> usize {
        PLIC_BASE + 0x200004 + context * 0x1000
    }

    fn read32(addr: usize) -> u32 {
        let raw = addr as *mut u32;
        // SAFETY: addr is always within the PLIC MMIO range.
        unsafe { core::ptr::read_volatile(raw) }
    }

    fn write32(addr: usize, val: u32) {
        let raw = addr as *mut u32;
        // SAFETY: addr is always within the PLIC MMIO range.
        unsafe { core::ptr::write_volatile(raw, val) };
    }
}
