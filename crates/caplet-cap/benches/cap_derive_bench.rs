//! Performance gate: capability derivation must be < 100 ns.
//!
//! docv1.md §20:
//! > `cap_derive_bench`: < 100 ns on `mps2-an500`
//!
//! On the hosted runner we measure the `cap_derive` fast
//! path. The QEMU profile run is the authoritative check.

use caplet_bench_harness::{report, time};
use caplet_cap::attenuation::DerivationOptions;
use caplet_cap::derivation::cap_derive;
use caplet_cap::handle::Handle;
use caplet_cap::permissions::Permissions;

#[derive(Copy, Clone)]
struct Dummy;

#[inline(never)]
fn derive_once() {
    let parent = Handle::<Dummy>::new(0, Default::default());
    let _ = cap_derive::<Dummy>(
        parent,
        Permissions::all(),
        Default::default(),
        DerivationOptions::default(),
    );
}

fn main() {
    let ns = time(100_000, derive_once);
    // 100 ns target with 50x slack for hosted CPU.
    report!("cap_derive", ns, 5_000);
}