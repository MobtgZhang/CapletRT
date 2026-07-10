# ADR-009: Minimal TCP for M2 — RFC 793 + Simple Timeout

Status: Accepted
Date: 2026-07-09
Supersedes: —

## Context

docv1.md §10.1.3 says: "if the home-grown TCP engine cannot
ship full NewReno in M2, fall back to RFC 793 + simple
timeout as the minimum viable; NewReno is deferred to M3."

## Decision

For the M2 industrial demo the TCP path is exercised only
by the DHCP client (which uses UDP anyway). The TCP stack
in `caplet-net::tcp` therefore:

1. Implements the three-way handshake (SYN, SYN-ACK, ACK).
2. Implements DATA + ACK with a simple per-segment timeout.
3. **Does not** implement fast retransmit, NewReno, or
   SACK. Loss recovery is by retransmission on timeout only.

If a future demo needs more aggressive loss recovery, the
contract above means we can swap the retransmission module
without changing the wire format or the state machine
enumeration.

## Consequences

- The TCP implementation is small (~150 LOC) and easy to
  reason about.
- Throughput on a lossy link is poor compared with NewReno,
  but the M2 demo does not measure TCP throughput.
- Kani can verify the state machine exhaustively.

## Alternatives considered

- **Full NewReno**: out of scope for M2; tracked in M3.
- **Use smoltcp**: rejected (see ADR-0006).