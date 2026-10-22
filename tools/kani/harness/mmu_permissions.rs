//! Cortex-A section-permission encoding.
//!
//! Property to prove: every kernel-facing [`PagePerms`] combination maps to
//! an ARMv7-A AP encoding that cannot grant user access unless the caller
//! explicitly requested it, and device mappings retain the strongly-ordered
//! attribute bits.

use caplet_cortex_a::mmu::APagePerms;
use caplet_kernel::mmu::PagePerms;

const AP_MASK: u32 = 0b11 << 10;
const AP_PRIV_RW: u32 = 0b01 << 10;
const AP_USER_RO: u32 = 0b10 << 10;
const AP_USER_RW: u32 = 0b11 << 10;
const DEVICE_ATTR_MASK: u32 = 0b1000;

#[kani::proof]
fn kernel_page_never_grants_user_access() {
    let raw = APagePerms::from_kernel(PagePerms {
        user_read: false,
        user_write: false,
        user_exec: kani::any(),
        device: kani::any(),
        shared: kani::any(),
    })
    .raw();
    assert_eq!(raw & AP_MASK, AP_PRIV_RW);
}

#[kani::proof]
fn read_only_user_page_maps_to_ap_user_ro() {
    let raw = APagePerms::from_kernel(PagePerms {
        user_read: true,
        user_write: false,
        user_exec: kani::any(),
        device: false,
        shared: kani::any(),
    })
    .raw();
    assert_eq!(raw & AP_MASK, AP_USER_RO);
}

#[kani::proof]
fn writable_user_page_maps_to_ap_user_rw() {
    let raw = APagePerms::from_kernel(PagePerms {
        user_read: true,
        user_write: true,
        user_exec: kani::any(),
        device: false,
        shared: kani::any(),
    })
    .raw();
    assert_eq!(raw & AP_MASK, AP_USER_RW);
}

#[kani::proof]
fn device_page_sets_strongly_ordered_attributes() {
    let raw = APagePerms::from_kernel(PagePerms {
        user_read: kani::any(),
        user_write: kani::any(),
        user_exec: kani::any(),
        device: true,
        shared: kani::any(),
    })
    .raw();
    assert_eq!(raw & DEVICE_ATTR_MASK, DEVICE_ATTR_MASK);
}
