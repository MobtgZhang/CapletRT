//! Performance gate: IPC send must be < 1 µs.
//!
//! docv1.md §20:
//! > `ipc_send_bench`: < 1 µs on `mps2-an500`
//!
//! We exercise the `caplet_sync::channel::Sender::send` path
//! with a tiny payload. On the hosted runner this measures
//! the SpinLock + VecDeque cost; the QEMU profile run is the
//! authoritative check.

use caplet_bench_harness::{report, time};
use caplet_sync::channel::Channel;

#[inline(never)]
fn send_one(tx: &caplet_sync::channel::Sender<u32>) {
    let _ = tx.send(0);
}

fn main() {
    let (tx, _rx) = Channel::<u32>::bounded(64);
    let ns = time(10_000, || send_one(&tx));
    // 1 µs = 1000 ns; allow 5x slack on hosted.
    report!("ipc_send", ns, 5_000);
}
