# `caplet-net`

User-space, pluggable, protocol-agnostic network stack.

## Modules

| Module       | Purpose                                            |
|--------------|----------------------------------------------------|
| `addr`       | `MacAddr`, `Ipv4Addr`, `Port`.                     |
| `interface`  | `NetworkInterface` trait + loopback implementation.|
| `socket`     | `SocketTable` — capability-mediated sockets.       |
| `syscall`    | `syscall_net` entry point.                         |
| `proto`      | IP / UDP / ICMP dispatch.                          |
| `eth`        | Ethernet frame parse.                              |
| `icmp`       | ICMP echo request/reply.                           |
| `tcp`        | Minimal TCP segment parser.                        |
| `dhcp`       | DHCP state machine.                                |
| `dns`        | Name encode/decode + static resolver.              |
| `cap`        | `SocketCap` — Drop-releasing handle.               |

## Hardware integration

Implement `NetworkInterface` against the platform's MAC (e.g.
STM32 ETH, nRF RADIO). The PHY layer lives in
`caplet-hal::phy` (see also `caplet-hal-stm32f4::phy_dp83848`).

## Status

Phase 7.1–7.5 complete. Real LWIP-quality sockets are out
of scope for the M2 industrial demo.