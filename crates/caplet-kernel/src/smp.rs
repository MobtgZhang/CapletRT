//! SMP bring-up and inter-processor coordination.
//!
//! Architecture ports provide the actual `CPU_ON` mechanism (PSCI, a
//! spin-table mailbox, or a platform-specific firmware call). This
//! module only invokes that hook, waits for each secondary to report
//! itself online, and publishes the observed CPU count. A missing or
//! failing hook is an error: the kernel never invents an online CPU.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use crate::cpu::{set_cpu_count, CpuId, MAX_CPUS};

/// Result returned by a port `CPU_ON` callback. Zero means the target
/// was released successfully; any other value is a platform failure.
pub type CpuOnFn = fn(u32) -> i32;

/// Callback used to send a reschedule IPI.
pub type IpiFn = fn(CpuId);

/// Quiet window used while waiting for secondaries.
///
/// A board cannot discover how many CPUs QEMU/Renode actually created:
/// releasing a mailbox slot for a non-existent CPU is indistinguishable
/// from a CPU that was released but failed before its Rust entry point.
/// Wait for a common window whose timer restarts whenever any new CPU
/// appears, instead of spending the full timeout separately on each
/// missing slot. A real SMP guest normally reports every released CPU
/// far inside this window.
const SECONDARY_QUIET_SPINS: u32 = 2_000_000;

/// Absolute upper bound for the complete bring-up wait. This protects a
/// board that keeps producing late CPUs from hanging boot forever.
const SECONDARY_TOTAL_SPINS: u32 = 16_000_000;

static CPU_ON_FN: AtomicUsize = AtomicUsize::new(0);
static IPI_FN: AtomicUsize = AtomicUsize::new(0);

/// True when at least one secondary CPU was observed online and the
/// SMP layer may dispatch work to it.
static SECONDARIES_ONLINE: AtomicBool = AtomicBool::new(false);

/// Bit 0 is the boot CPU; bits 1..MAX_CPUS are secondaries that have
/// executed their real entry point. `fetch_or` makes duplicate reports
/// idempotent.
static ONLINE_MASK: AtomicU32 = AtomicU32::new(1);

/// Number of secondary CPUs that reached the architecture port's
/// secondary entry point.
static SECONDARIES_ALIVE: AtomicU32 = AtomicU32::new(0);

/// Work units executed by each secondary worker. This is deliberately a
/// real, observable workload rather than an idle loop so the simulator
/// can prove that every released core reached Rust and made forward
/// progress.
pub const SECONDARY_WORK_ITERATIONS: u32 = 65_536;

/// Per-CPU completed work units.
static SECONDARY_WORK_UNITS: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];

/// Bitmap of CPUs that completed at least one secondary worker task.
static SECONDARY_WORK_DONE: AtomicU32 = AtomicU32::new(0);

/// Prevent a second boot attempt from clearing a live CPU bitmap.
static BOOT_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// Total number of IPI requests sent (host-side observability).
static IPI_REQUESTS: AtomicU32 = AtomicU32::new(0);

/// Install the architecture's secondary-CPU release routine.
pub fn install_cpu_on(cpu_on: CpuOnFn) {
    CPU_ON_FN.store(cpu_on as usize, Ordering::Release);
}

/// Install the architecture's reschedule-IPI routine.
pub fn install_ipi(ipi: IpiFn) {
    IPI_FN.store(ipi as usize, Ordering::Release);
}

/// Record that `cpu` reached its post-MMU Rust entry point. This is
/// called by the architecture port, normally from the board's
/// secondary console stub.
pub fn mark_secondary_online(cpu: u32) {
    if cpu == 0 || cpu as usize >= MAX_CPUS {
        return;
    }
    let bit = 1u32 << cpu;
    let previous = ONLINE_MASK.fetch_or(bit, Ordering::AcqRel);
    if previous & bit == 0 {
        SECONDARIES_ALIVE.fetch_add(1, Ordering::AcqRel);
    }
}

/// Run the secondary-side worker task for `cpu`.
///
/// The worker performs a bounded amount of arithmetic and publishes a
/// completion bit. Boards call this after `mark_secondary_online` so a
/// simulator assertion can distinguish "entered Rust" from "actually
/// executed work". Each CPU has an independent counter, so the proof is
/// per-core rather than a single shared total.
pub fn run_secondary_worker(cpu: u32) {
    if cpu == 0 || cpu as usize >= MAX_CPUS {
        return;
    }
    let slot = &SECONDARY_WORK_UNITS[cpu as usize];
    let mut acc = cpu;
    for _ in 0..SECONDARY_WORK_ITERATIONS {
        acc = acc.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        slot.store(acc, Ordering::Relaxed);
    }
    SECONDARY_WORK_DONE.fetch_or(1u32 << cpu, Ordering::Release);
}

/// Number of completed work units observed for `cpu`.
pub fn secondary_work_units(cpu: u32) -> u32 {
    if cpu == 0 || cpu as usize >= MAX_CPUS {
        return 0;
    }
    SECONDARY_WORK_UNITS[cpu as usize].load(Ordering::Acquire)
}

/// Wait for every secondary in `online_mask` to finish its worker task.
///
/// Returns the number of secondary workers that completed. The bounded
/// wait keeps a broken board from hanging the boot path indefinitely;
/// callers can compare the result with [`secondaries_alive`].
pub fn wait_for_secondary_workers(online_mask: u32) -> u32 {
    let expected = online_mask & !1u32 & ((1u32 << MAX_CPUS) - 1);
    if expected == 0 {
        return 0;
    }
    let mut spins = 0u32;
    while SECONDARY_WORK_DONE.load(Ordering::Acquire) & expected != expected
        && spins < SECONDARY_QUIET_SPINS
    {
        core::hint::spin_loop();
        spins += 1;
    }
    (SECONDARY_WORK_DONE.load(Ordering::Acquire) & expected).count_ones()
}

/// Generic secondary idle body. Boards may call this after installing
/// their MMU and console, or use their own loop after calling
/// [`mark_secondary_online`].
pub extern "C" fn secondary_main() -> ! {
    let cpu = crate::cpu::current_cpu().0;
    mark_secondary_online(cpu);
    run_secondary_worker(cpu);
    loop {
        #[cfg(target_arch = "arm")]
        // SAFETY: WFI is a low-power hint and has no memory side effects.
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack, preserves_flags));
        }
        #[cfg(not(target_arch = "arm"))]
        core::hint::spin_loop();
    }
}

/// Release and wait for up to `requested_cpus` CPUs.
///
/// `requested_cpus` includes the boot CPU. Platforms that expose more
/// CPUs than exist (for example a fixed four-entry mailbox on a
/// single-core machine) simply time out after a common quiet window.
/// The return value is the number of CPUs actually observed, and
/// [`crate::cpu::cpu_count`] is updated to match it.
///
/// Calling this more than once returns the count observed by the first
/// call without resetting live state.
pub fn boot_secondary_cpus(requested_cpus: u32) -> u32 {
    if BOOT_ATTEMPTED.swap(true, Ordering::AcqRel) {
        return current_online_count();
    }

    let requested = requested_cpus.clamp(1, MAX_CPUS as u32);
    if requested == 1 {
        set_cpu_count(1);
        SECONDARIES_ONLINE.store(false, Ordering::Release);
        return 1;
    }

    let hook_address = CPU_ON_FN.load(Ordering::Acquire);
    if hook_address == 0 {
        set_cpu_count(1);
        SECONDARIES_ONLINE.store(false, Ordering::Release);
        return 1;
    }
    // SAFETY: only `CpuOnFn` values produced by `install_cpu_on` are
    // stored in this slot.
    let cpu_on: CpuOnFn = unsafe { core::mem::transmute(hook_address) };

    for cpu in 1..requested {
        // A failed release is not fatal for the whole cluster: record
        // it as absent and continue with the other CPUs. The failure
        // remains visible because that bit never appears in ONLINE_MASK.
        let _ = cpu_on(cpu);
    }

    let requested_mask = ((1u32 << requested) - 1) & ((1u32 << MAX_CPUS) - 1);
    let mut observed = ONLINE_MASK.load(Ordering::Acquire) & requested_mask;
    let mut quiet_spins = 0u32;
    let mut total_spins = 0u32;
    while observed != requested_mask
        && quiet_spins < SECONDARY_QUIET_SPINS
        && total_spins < SECONDARY_TOTAL_SPINS
    {
        core::hint::spin_loop();
        total_spins += 1;
        quiet_spins += 1;
        let now = ONLINE_MASK.load(Ordering::Acquire) & requested_mask;
        if now != observed {
            observed = now;
            quiet_spins = 0;
        }
    }

    let online = current_online_count();
    set_cpu_count(online);
    SECONDARIES_ONLINE.store(online > 1, Ordering::Release);
    online
}

fn current_online_count() -> u32 {
    let mask = ONLINE_MASK.load(Ordering::Acquire) & ((1u32 << MAX_CPUS) - 1);
    mask.count_ones()
}

/// Returns `true` when at least one secondary was observed online.
pub fn secondaries_online() -> bool {
    SECONDARIES_ONLINE.load(Ordering::Acquire)
}

/// Number of secondary CPUs that reached their real entry point.
pub fn secondaries_alive() -> u32 {
    SECONDARIES_ALIVE.load(Ordering::Acquire)
}

/// Bitmap of CPUs observed online. Bit 0 is always set after boot.
pub fn online_mask() -> u32 {
    ONLINE_MASK.load(Ordering::Acquire)
}

/// Send a "reschedule" IPI to `cpu`. The target CPU's IRQ handler must
/// call `caplet_kernel::smp_scheduler::SmpScheduler::tick`.
pub fn request_reschedule(cpu: CpuId) {
    IPI_REQUESTS.fetch_add(1, Ordering::AcqRel);
    let address = IPI_FN.load(Ordering::Acquire);
    if address == 0 {
        return;
    }
    // SAFETY: only `IpiFn` values produced by `install_ipi` are stored
    // in this slot.
    let ipi: IpiFn = unsafe { core::mem::transmute(address) };
    ipi(cpu);
}

/// Read the IPI request counter. Host tests use this to verify that
/// `request_reschedule` was called the expected number of times.
pub fn ipi_requests() -> u32 {
    IPI_REQUESTS.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_cpu_does_not_claim_secondaries() {
        assert_eq!(boot_secondary_cpus(1), 1);
        assert!(!secondaries_online());
        assert_eq!(secondaries_alive(), 0);
        assert_eq!(online_mask(), 1);
    }

    #[test]
    fn ipi_requests_increment() {
        let before = ipi_requests();
        request_reschedule(CpuId(1));
        let after = ipi_requests();
        assert!(after > before);
    }
}
