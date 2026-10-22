//! Cortex-A spin-table secondary bring-up.
//!
//! QEMU `vexpress-a9`, QEMU `cubieboard` and the Renode platforms in
//! this repository release every configured CPU at the same physical
//! reset vector. The boot CPU initialises BSS, the MMU and the UART,
//! then publishes a per-CPU stack/top and entry address in a static
//! mailbox. Secondary CPUs poll that mailbox from their reset stub and
//! enter Rust only after the primary has finished cluster setup.

use core::sync::atomic::{fence, Ordering};

use crate::mmu;

/// Value written by the boot CPU to release a secondary.
pub const SECONDARY_RELEASE_MAGIC: u32 = 0xC0DE_0001;

/// Number of mailbox slots, matching the port's architecture limit.
pub const SECONDARY_CPU_SLOTS: usize = 4;

/// Mailbox layout consumed by the board reset stubs. Keep the field
/// order and the 16-byte stride in sync with the assembly in the board
/// crates.
#[repr(C)]
pub struct SecondaryMailbox {
    /// [`SECONDARY_RELEASE_MAGIC`] once the CPU may start.
    pub release: u32,
    /// Stack pointer installed before the Rust entry point runs.
    pub stack_top: u32,
    /// Shared L1 section table address.
    pub l1_table: u32,
    /// Post-MMU entry point (`secondary_entry`).
    pub entry: u32,
}

impl SecondaryMailbox {
    const fn empty() -> Self {
        Self {
            release: 0,
            stack_top: 0,
            l1_table: 0,
            entry: 0,
        }
    }
}

/// Mailbox symbol referenced by board reset stubs.
#[no_mangle]
pub static mut CAPLET_SECONDARY_MAILBOX: [SecondaryMailbox; SECONDARY_CPU_SLOTS] = [
    SecondaryMailbox::empty(),
    SecondaryMailbox::empty(),
    SecondaryMailbox::empty(),
    SecondaryMailbox::empty(),
];

/// Prepare a secondary without releasing it.
///
/// `entry` must be a physical address that is valid with the MMU off
/// and with the shared identity map enabled. `stack_top` must point to
/// the top of a private stack (16-byte aligned).
pub fn prepare_secondary(cpu: u32, entry: u32, stack_top: u32, l1_table: u32) {
    if cpu == 0 || cpu as usize >= SECONDARY_CPU_SLOTS {
        return;
    }
    // SAFETY: boot-time setup runs while the CPU bitmap is single-core
    // and this CPU's slot is not yet visible to the secondary.
    unsafe {
        let slot = core::ptr::addr_of_mut!(CAPLET_SECONDARY_MAILBOX[cpu as usize]);
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*slot).stack_top), stack_top);
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*slot).l1_table), l1_table);
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*slot).entry), entry);
    }
}

/// Release a prepared secondary. Returns 0 on success, PSCI-style
/// `-1` (`NOT_SUPPORTED`) for an invalid or unprepared slot.
pub fn release_secondary(cpu: u32) -> i32 {
    if cpu == 0 || cpu as usize >= SECONDARY_CPU_SLOTS {
        return -1;
    }
    // SAFETY: the slot was prepared by `prepare_secondary` and the
    // secondary is still polling with MMU off.
    unsafe {
        let slot = core::ptr::addr_of_mut!(CAPLET_SECONDARY_MAILBOX[cpu as usize]);
        let stack = core::ptr::read_volatile(core::ptr::addr_of!((*slot).stack_top));
        let table = core::ptr::read_volatile(core::ptr::addr_of!((*slot).l1_table));
        let entry = core::ptr::read_volatile(core::ptr::addr_of!((*slot).entry));
        if stack == 0 || table == 0 || entry == 0 {
            return -1;
        }
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*slot).release),
            SECONDARY_RELEASE_MAGIC,
        );
    }
    fence(Ordering::SeqCst);
    0
}

/// Read Aff0 from MPIDR on ARMv7-A. This is safe to call from any
/// exception level and does not depend on TPIDR registers.
pub fn current_cpu_id() -> u32 {
    #[cfg(target_arch = "arm")]
    {
        let mpidr: u32;
        // SAFETY: MPIDR is a read-only identification register.
        unsafe {
            core::arch::asm!(
                "mrc p15, 0, {mpidr}, c0, c0, 5",
                mpidr = out(reg) mpidr,
                options(nomem, nostack, preserves_flags)
            );
        }
        mpidr & 0xff
    }
    #[cfg(not(target_arch = "arm"))]
    {
        0
    }
}

/// Rust entry point reached by the board reset stub after it installs
/// the per-CPU stack from the mailbox.
///
/// # Safety
///
/// Must run on a secondary CPU, with `l1_table` containing the shared
/// identity map and the board's `caplet_secondary_rust` symbol linked.
#[no_mangle]
pub unsafe extern "C" fn secondary_entry(cpu: u32, l1_table: u32) -> ! {
    // SAFETY: guaranteed by the boot contract above.
    unsafe { mmu::enable_table(l1_table) };

    unsafe extern "C" {
        fn caplet_secondary_rust(cpu: u32) -> !;
    }
    // SAFETY: the board provides this symbol and owns the console/GIC
    // initialisation for its platform.
    unsafe { caplet_secondary_rust(cpu) }
}

/// Install the kernel's CPU identification and IPI hooks.
pub fn install_kernel_hooks() {
    caplet_kernel::cpu::set_current_cpu_fn(current_cpu_id_for_kernel);
    caplet_kernel::smp::install_cpu_on(release_secondary);
    caplet_kernel::smp::install_ipi(send_reschedule_ipi);
}

fn current_cpu_id_for_kernel() -> caplet_kernel::CpuId {
    caplet_kernel::CpuId::new(current_cpu_id())
}

fn send_reschedule_ipi(cpu: caplet_kernel::CpuId) {
    // SAFETY: the GIC distributor is identity-mapped on both supported
    // A-profile boards. SGI 0 is reserved for rescheduling.
    unsafe {
        if cpu.0 == 0 {
            crate::gic::sgi_self(0);
        } else if cpu.0 < 8 {
            crate::gic::send_sgi(0, 1u8 << cpu.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_cpu_release_is_rejected() {
        assert_eq!(release_secondary(0), -1);
        assert_eq!(release_secondary(4), -1);
    }

    #[test]
    fn mailbox_stride_is_sixteen_bytes() {
        assert_eq!(core::mem::size_of::<SecondaryMailbox>(), 16);
    }
}
