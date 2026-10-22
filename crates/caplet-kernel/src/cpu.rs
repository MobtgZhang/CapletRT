//! CPU identification and per-CPU state for SMP support.
//!
//! CapletRT targets embedded systems where the number of CPUs is
//! statically known. On Cortex-M the constant is `1`; on Cortex-A we
//! configure it at boot time via `set_cpu_count`.
//!
//! The `CpuId` newtype is used in place of `u32` everywhere we need to
//! disambiguate per-CPU state. It is intentionally `Copy` so the
//! caller does not have to think about lifetimes.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Maximum number of CPUs CapletRT supports. This is a hard upper
/// bound because all per-CPU arrays are sized at compile time. v1
/// targets small embedded SMP (typically 2-4 cores).
pub const MAX_CPUS: usize = 4;

/// Reason for an inter-processor interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpiReason {
    /// Request the target CPU to reschedule.
    Reschedule = 0,
    /// TLB shootdown (the payload carries the vaddr in `current_ipi_arg`).
    TlbShootdown = 1,
    /// Halt the target CPU.
    Halt = 2,
}

/// A 32-bit argument associated with the most recent IPI sent. On
/// Cortex-A this is set by the sender and read by the receiver in the
/// SGI handler before clearing the slot.
pub static CURRENT_IPI_ARG: AtomicU32 = AtomicU32::new(0);

/// Logical CPU id (0..MAX_CPUS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CpuId(pub u32);

impl CpuId {
    /// Construct a CPU id. Saturates at `MAX_CPUS - 1`.
    pub const fn new(id: u32) -> Self {
        if id as usize >= MAX_CPUS {
            Self((MAX_CPUS - 1) as u32)
        } else {
            Self(id)
        }
    }

    /// Index into per-CPU arrays.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Iteration over all valid CPU ids.
    pub fn all() -> core::array::IntoIter<CpuId, MAX_CPUS> {
        let arr: [CpuId; MAX_CPUS] = [CpuId(0), CpuId(1), CpuId(2), CpuId(3)];
        // For const fns we'd use a literal, but IntoIter is not const.
        // The unwrap is safe because the array has exactly MAX_CPUS elements.
        <[CpuId; MAX_CPUS] as IntoIterator>::into_iter(arr)
    }
}

impl core::fmt::Display for CpuId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "cpu{}", self.0)
    }
}

/// Number of online CPUs. Defaults to 1 (single-CPU fallback) and is
/// bumped by `set_cpu_count` at boot. Read by the scheduler to know how
/// many per-CPU run queues to populate.
static ONLINE_CPUS: AtomicU32 = AtomicU32::new(1);

/// Read the number of online CPUs.
pub fn cpu_count() -> u32 {
    ONLINE_CPUS.load(Ordering::Acquire)
}

/// Set the number of online CPUs. Must be called during boot, before
/// the scheduler starts dispatching tasks. Saturates at `MAX_CPUS`.
pub fn set_cpu_count(n: u32) {
    let capped = (n as usize).clamp(1, MAX_CPUS);
    ONLINE_CPUS.store(capped as u32, Ordering::Release);
}

/// Port-provided CPU identification routine. Stored as an address so
/// this module keeps working on targets where atomics and function
/// pointers have different widths.
static CURRENT_CPU_FN: AtomicUsize = AtomicUsize::new(0);

/// Install the architecture port's CPU identification routine.
///
/// MPIDR (`arm`) and `mhartid` (`riscv`) are privileged registers and
/// cannot be read by the generic kernel on every supported target.
/// Cortex-M and host builds keep the CPU-0 fallback.
pub fn set_current_cpu_fn(identify: fn() -> CpuId) {
    CURRENT_CPU_FN.store(identify as usize, Ordering::Release);
}

/// Identify the current CPU.
///
/// The default is CPU 0, which is correct for single-core Cortex-M
/// targets and host tests. SMP architecture ports install their own
/// routine with [`set_current_cpu_fn`] before releasing secondaries.
pub fn current_cpu() -> CpuId {
    let address = CURRENT_CPU_FN.load(Ordering::Acquire);
    if address == 0 {
        return CpuId(0);
    }
    // SAFETY: only function pointers produced by `set_current_cpu_fn`
    // are stored in this slot, and code cannot be unloaded in the
    // bare-metal runtime.
    let identify: fn() -> CpuId = unsafe { core::mem::transmute(address) };
    identify()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    /// `ONLINE_CPUS` is process-global and cargo runs tests in parallel
    /// threads, so tests that mutate it must not overlap.
    /// `cpu_count_default_is_one` asserts on the pristine initial value,
    /// which any concurrent `set_cpu_count` call would otherwise clobber.
    ///
    /// This crate is `#![no_std]` and has no `std::sync::Mutex`, so use a
    /// minimal spin lock built from `core::sync::atomic`. It is only used
    /// by these tests, which never block inside the critical section.
    struct TestLock(AtomicBool);

    impl TestLock {
        const fn new() -> Self {
            Self(AtomicBool::new(false))
        }

        fn acquire(&self) -> TestGuard<'_> {
            while self
                .0
                .compare_exchange_weak(
                    false,
                    true,
                    AtomicOrdering::Acquire,
                    AtomicOrdering::Relaxed,
                )
                .is_err()
            {
                core::hint::spin_loop();
            }
            TestGuard(&self.0)
        }
    }

    struct TestGuard<'a>(&'a AtomicBool);

    impl Drop for TestGuard<'_> {
        fn drop(&mut self) {
            self.0.store(false, AtomicOrdering::Release);
        }
    }

    static CPU_COUNT_TEST_LOCK: TestLock = TestLock::new();

    #[test]
    fn cpu_id_saturates() {
        assert_eq!(CpuId::new(0).0, 0);
        assert_eq!(CpuId::new(3).0, 3);
        assert_eq!(CpuId::new(99).0, (MAX_CPUS - 1) as u32);
    }

    #[test]
    fn cpu_count_default_is_one() {
        let _guard = CPU_COUNT_TEST_LOCK.acquire();
        // The static initialises to 1. Other tests in this module that
        // call `set_cpu_count` hold the same lock and restore the value,
        // so observing the default here is deterministic.
        assert_eq!(cpu_count(), 1);
    }

    #[test]
    fn cpu_count_set_and_read() {
        let _guard = CPU_COUNT_TEST_LOCK.acquire();
        set_cpu_count(2);
        assert_eq!(cpu_count(), 2);
        set_cpu_count(99);
        assert_eq!(cpu_count(), MAX_CPUS as u32);
        set_cpu_count(0);
        assert_eq!(cpu_count(), 1);
        // Leaves ONLINE_CPUS == 1, matching the documented default.
    }

    #[test]
    fn all_iterates() {
        let ids: alloc::vec::Vec<u32> = CpuId::all().map(|c| c.0).collect();
        assert_eq!(ids, alloc::vec![0, 1, 2, 3]);
    }
}
