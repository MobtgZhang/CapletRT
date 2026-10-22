//! Tiny std-only bench harness used by the M2 performance gate.
//!
//! Provides a `time` helper (wall-clock nanoseconds) and a
//! `report!` macro. Used by:
//!
//! * `context_switch_bench`  — `< 2 µs` target.
//! * `cap_derive_bench`      — `< 100 ns` target.
//! * `ipc_send_bench`        — `< 1 µs` target.
//!
//! On the hosted runner these benchmarks exercise the kernel
//! code paths in software; the authoritative measurements are
//! produced under QEMU and recorded by
//! `tests/qemu-cm4/run.sh --profile`.

use std::time::Instant;

/// Time a closure `iters` times and return ns/op.
pub fn time<F: FnMut()>(iters: u64, mut f: F) -> u64 {
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = start.elapsed();
    elapsed.as_nanos() as u64 / iters.max(1)
}

/// Print a benchmark result.
#[macro_export]
macro_rules! report {
    ($name:expr, $ns:expr) => {{
        println!("bench {:<24} : {:>6} ns/op", $name, $ns);
    }};
    ($name:expr, $ns:expr, $budget:expr) => {{
        let ns = $ns;
        let budget = $budget;
        let status = if ns <= budget { "PASS" } else { "FAIL" };
        println!(
            "bench {:<24} : {:>6} ns/op (budget {:>6} ns) [{}]",
            $name, ns, budget, status
        );
        if ns > budget {
            eprintln!(
                "performance gate FAILED: {} ({} ns > {} ns)",
                $name, ns, budget
            );
            std::process::exit(2);
        }
    }};
}
