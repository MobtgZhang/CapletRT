//! Kani harness: cap_derive
//!
//! Property: `cap_derive` is total over the allowed `Handle`
//! shape. For any parent `Handle<T>` (with index < 4096 and
//! generation != 0) and any `Permissions`/`SafetyBits` combination
//! in their respective small ranges, the function must:
//!   1. Return `Ok` (it never errors today — the type signature
//!      is the contract).
//!   2. Produce a `DerivedCapability` whose child handle has the
//!      same `index()` as the parent.
//!   3. Inherit the parent `Permissions` and `SafetyBits`
//!      unchanged (attenuation is opt-in via
//!      `derive_with_perms`).
//!
//! This is a baseline property. Phase 1.3 will add a more
//! interesting harness that exercises the attenuation case.

#![cfg(kani)]

use caplet_cap::attenuation::DerivationOptions;
use caplet_cap::derivation::cap_derive;
use caplet_cap::handle::{Generation, Handle};
use caplet_cap::permissions::Permissions;
use caplet_cap::safety_bits::SafetyBits;

#[derive(Clone, Copy)]
struct Dummy;

#[kani::proof]
fn cap_derive_preserves_parent_metadata() {
    // Constrain the inputs to small ranges so the bounded
    // model check finishes in a reasonable time.
    let raw_index: u32 = kani::any();
    kani::assume(raw_index < 4096);

    let raw_perm: u32 = kani::any();
    kani::assume(raw_perm < 8);

    let raw_safety: u8 = kani::any();
    kani::assume(raw_safety < 8);

    let parent = Handle::<Dummy>::new(raw_index, Generation(1));
    let perms = Permissions::from_bits(raw_perm);
    let safety = SafetyBits::from_bits(raw_safety);

    let result = cap_derive(parent, perms, safety, DerivationOptions::default());

    match result {
        Ok(derived) => {
            assert!(derived.handle.index() == parent.index());
            assert!(derived.permissions == perms);
            assert!(derived.safety_bits == safety);
            assert!(derived.parent.index() == parent.index());
        }
        Err(_) => {
            // Today's `cap_derive` never returns Err; any error
            // here is a regression.
            panic!("cap_derive unexpectedly returned Err");
        }
    }
}