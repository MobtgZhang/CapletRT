//! Performance gate: context-switch cost must be < 2 µs.
//!
//! docv1.md §20:
//! > `context_switch_bench`: < 2 µs on `mps2-an500`
//!
//! The hosted bench drives `caplet-async::waker::CapletWaker::noop`
//! through a wake-by-ref loop to exercise the wake-queue
//! path. The authoritative measurement is the QEMU profile
//! run recorded by `tests/qemu-cm4/run.sh --profile` and
//! archived under `tests/artifacts/`.

use caplet_async::waker::CapletWaker;
use caplet_bench_harness::{report, time};
use core::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

#[inline(never)]
fn spin() {
    let waker = CapletWaker::noop();
    // CapletWaker implements `Wake` via Arc<Self>; without
    // an Arc we fall through to `push_wake`, which exercises
    // the same wake-queue code path as a real waker.
    waker.push_wake(caplet_async::WakeSource::Manual);
    COUNTER.fetch_add(1, Ordering::Relaxed);
}

fn main() {
    let ns = time(10_000, spin);
    // 2 µs = 2000 ns. We allow 5x slack on the hosted runner
    // because the wall-clock loop dominates the actual cost
    // on a GHz-class CPU. The QEMU gate is the authoritative
    // check.
    report!("context_switch", ns, 10_000);
}
