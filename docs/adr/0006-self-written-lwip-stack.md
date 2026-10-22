# ADR-006: Self-Written LWIP-Style Network Stack

Status: Accepted
Date: 2026-07-09
Supersedes: —

## Context

The M2 industrial demo includes an MQTT-SN client that must
run over UDP on the loopback interface (and, on real hardware,
over Ethernet via the DP83848 PHY). Two options are available:

1. **Vendor LWIP port** (e.g. lwIP-rs, smoltcp, embassy-net).
2. **Hand-rolled LWIP-style stack** that we control end-to-end.

Smoltcp is an excellent fit but its socket layer assumes a
BSD-style API and its socket table is not capability-aware.
LWIP itself is a vast C codebase whose integration into Rust
is fragile.

## Decision

We implement a **minimal, capability-aware, LWIP-style stack**
in `caplet-net`. The stack owns:

- Ethernet (`eth.rs`): frame parse/encode.
- IPv4 + ICMP + UDP + TCP (`proto.rs`, `icmp.rs`, `tcp.rs`).
- DHCP (`dhcp.rs`) and DNS (`dns.rs`).
- A `NetworkInterface` trait with a loopback implementation
  and (on STM32F4) an Ethernet MAC binding.
- A `SocketTable` whose slots are exposed as `Cap<Socket>` via
  the `cap` module.

The TCP implementation is intentionally minimal — enough to
model the handshake, not to be a production-grade TCP stack.
The industrial demo does not need more than UDP.

## Consequences

- The entire stack is `no_std` and `unsafe`-free except for
  the hardware MAC DMA ring.
- Capability revocation works at the socket level: when the
  `Cap<Socket>` is dropped, the underlying slot is reused.
- We do not depend on an external crate that could quietly
  change behaviour between releases.

## Alternatives considered

- **smoltcp**: rejected because the BSD socket API does not
  fit our capability model.
- **lwIP via FFI**: rejected because it doubles the TCB and
  complicates Kani verification.
- **Raw `unsafe` DMA + ad-hoc parsing**: rejected because
  the parsing layer is exactly what we want to Kani-verify.