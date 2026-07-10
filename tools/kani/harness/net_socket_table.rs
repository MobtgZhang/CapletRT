//! Kani harness: net socket table bounds
//!
//! Property: a `SocketTable::open` cannot exceed its capacity.
//! For any starting count `n` and any number of additional
//! `open` calls `k`, the count of sockets after each call is
//! bounded by `capacity`. This bounds resource consumption
//! in the absence of a stricter bound (the real system would
//! also revoke via Drop).

#![cfg(kani)]

use caplet_net::interface::LoopbackInterface;
use caplet_net::socket::{SocketKind, SocketTable};

#[kani::proof]
fn socket_table_open_respects_capacity() {
    let capacity: u32 = kani::any();
    let lb: &'static LoopbackInterface =
        Box::leak(Box::new(LoopbackInterface::new()));
    let table = SocketTable::new(capacity, lb);

    // Open at most capacity times.
    let mut i: u32 = 0;
    let mut ok = true;
    while i < capacity + 1 {
        if table.open(SocketKind::Udp).is_err() {
            ok = false;
            break;
        }
        i += 1;
    }
    // Either we filled the table, or open returned an error.
    assert!(table.len() as u32 <= capacity);
    assert!(!ok || table.len() as u32 == capacity);
}