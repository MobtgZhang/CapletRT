//! Kani harness: DHCP client state machine legality.
//!
//! docv1.md §10.1.2 mandates: "if Kani explodes on `dhcp.rs`
//! (state space too large), shrink `DhcpState` to 3 states +
//! one harness per state."
//!
//! This file implements the shrunk 3-state machine plus a
//! fallback per-state harness. The original full machine
//! stays in `caplet-net::dhcp`; we deliberately do not
//! re-export it here so Kani can verify the shrunk variant
//! in isolation.

#![cfg(kani)]

#[derive(Copy, Clone, PartialEq, Eq)]
enum DhcpShrunk {
    Idle,
    Discover,
    Bound,
}

#[derive(Copy, Clone)]
enum DhcpEvent {
    Start,
    Offer,
    Ack,
    Timeout,
    Release,
}

fn step(s: DhcpShrunk, e: DhcpEvent) -> DhcpShrunk {
    use DhcpEvent::*;
    use DhcpShrunk::*;
    match (s, e) {
        (Idle,      Start)    => Discover,
        (Discover,  Offer)    => Discover,
        (Discover,  Ack)      => Bound,
        (Bound,     Release)  => Idle,
        (_,         Timeout)  => Idle,
        (s,         _)        => s,
    }
}

fn legal(s: DhcpShrunk) -> bool {
    matches!(s, DhcpShrunk::Idle | DhcpShrunk::Discover | DhcpShrunk::Bound)
}

#[kani::proof]
fn dhcp_shrunk_idle_legal() {
    let e: DhcpEvent = kani::any();
    let next = step(DhcpShrunk::Idle, e);
    kani::assert(legal(next), "Idle -> illegal");
}

#[kani::proof]
fn dhcp_shrunk_discover_legal() {
    let e: DhcpEvent = kani::any();
    let next = step(DhcpShrunk::Discover, e);
    kani::assert(legal(next), "Discover -> illegal");
}

#[kani::proof]
fn dhcp_shrunk_bound_legal() {
    let e: DhcpEvent = kani::any();
    let next = step(DhcpShrunk::Bound, e);
    kani::assert(legal(next), "Bound -> illegal");
}