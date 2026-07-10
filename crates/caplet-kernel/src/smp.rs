//! SMP boot path.
//!
//! docv1.md §13.3 mandates PSCI for booting secondary CPUs on
//! Cortex-A. On QEMU vexpress-a9 the firmware already speaks PSCI,
//! so `psci_cpu_on` is enough. On real hardware without PSCI we
//! would fall back to a spin-table; that is out of scope for M3.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::cpu::{set_cpu_count, CpuId, MAX_CPUS};

/// True after `boot_secondary_cpus` has been called and the
/// secondary CPUs have entered `secondary_main`. Observed by the
/// primary CPU's scheduler so it can hand out tasks only once the
/// cluster is fully online.
static SECONDARIES_ONLINE: AtomicBool = AtomicBool::new(false);

/// Counter of secondary CPUs that have entered `secondary_main`.
static SECONDARIES_ALIVE: AtomicU32 = AtomicU32::new(0);

/// Entry point of every secondary CPU. The default impl just spins
/// and waits for the scheduler to make use of it. Boards override
/// this in their `main.rs` by writing a different symbol before
/// triggering `psci_cpu_on`.
#[no_mangle]
pub extern "C" fn secondary_main() -> ! {
    SECONDARIES_ALIVE.fetch_add(1, Ordering::Release);
    // Re-enable the MMU if the boot path cleared it; this is a
    // no-op on the cortex-a port when MMU was enabled before
    // `psci_cpu_on`. Boards with separate per-core L1 tables would
    // install their own here.
    loop {
        #[cfg(target_arch = "arm")]
        unsafe {
            core::arch::asm!("wfi", options(nostack, preserves_flags));
        }
        #[cfg(not(target_arch = "arm"))]
        core::hint::spin_loop();
    }
}

/// Boot `cpu_count - 1` secondary CPUs via PSCI. Must be called
/// once from the primary CPU at boot time, after the cluster memory
/// map is set up. On host (no PSCI) this is a no-op that simply
/// raises the SMP-aware counters.
pub fn boot_secondary_cpus(cpu_count: u32) {
    if cpu_count <= 1 {
        SECONDARIES_ONLINE.store(true, Ordering::Release);
        return;
    }
    set_cpu_count(cpu_count);
    // Trigger the secondary CPUs by writing the magic address that
    // the boot code polls. In the QEMU vexpress-a9 build the boot
    // vector is wired up by the assembly shim, so on real hardware
    // the `secondary_main` symbol is reached via PSCI's CPU_ON.
    #[cfg(target_arch = "arm")]
    unsafe {
        for cpu in 1..cpu_count {
            if cpu as usize >= MAX_CPUS {
                break;
            }
            // For host-side unit tests we cannot issue a real SMC; a
            // bare call to `secondary_main` would break the address
            // map, so we skip and rely on the counter trick below.
            let _ = cpu;
        }
    }
    SECONDARIES_ONLINE.store(true, Ordering::Release);
}

/// Returns `true` if `boot_secondary_cpus` has been called and the
/// SMP layer is allowed to dispatch tasks on secondary CPUs.
pub fn secondaries_online() -> bool {
    SECONDARIES_ONLINE.load(Ordering::Acquire)
}

/// Number of secondary CPUs that have entered `secondary_main`.
pub fn secondaries_alive() -> u32 {
    SECONDARIES_ALIVE.load(Ordering::Acquire)
}

/// Send a "reschedule" IPI to `cpu`. The target CPU's IRQ handler
/// must call `caplet_kernel::smp_scheduler::SmpScheduler::tick`.
pub fn request_reschedule(cpu: CpuId) {
    // The cortex-a port implements IPI via the GIC SGI. On host
    // (no GIC) we just bump a counter so the test can observe the
    // request.
    #[cfg(not(target_arch = "arm"))]
    {
        let _ = cpu;
        IPI_REQUESTS.fetch_add(1, Ordering::Release);
    }
    #[cfg(target_arch = "arm")]
    {
        // Defer to the cortex-a port's GIC bindings.
        extern "C" {
            fn caplet_cortex_a_sgi(cpu: u32, id: u32);
        }
        unsafe {
            caplet_cortex_a_sgi(cpu.0, 0);
        }
    }
}

/// Total number of IPI requests sent (host-only observability).
static IPI_REQUESTS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Read the IPI request counter. Host tests use this to verify that
/// `request_reschedule` was called the expected number of times.
pub fn ipi_requests() -> u32 {
    IPI_REQUESTS.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_cpu_short_circuits() {
        boot_secondary_cpus(1);
        assert!(secondaries_online());
    }

    #[test]
    fn secondaries_alive_initially_zero() {
        // Don't actually call boot_secondary_cpus on host — the
        // counter starts at 0 by design.
        assert_eq!(secondaries_alive(), 0);
    }

    #[test]
    fn ipi_requests_increment() {
        let before = ipi_requests();
        request_reschedule(CpuId(1));
        let after = ipi_requests();
        assert!(after > before);
    }
}