# Architecture Decision Records

This directory holds CapletRT's ADRs. Each document records an
important architectural decision, the context that led to it,
and the consequences of the choice.

## Index

| #    | Title                                                 | Status   |
|------|-------------------------------------------------------|----------|
| 0001 | Capability as the Single Authorisation Primitive     | Accepted |
| 0002 | Async/Await as the Default Concurrency Primitive      | Accepted |
| 0003 | Tickless Idle Powered by SysTick + Software Counter   | Accepted |
| 0004 | Resource Budgets (Memory, CPU, Message) Per Task      | Accepted |
| 0005 | VFS with Capability-Mediated Handles                  | Accepted |
| 0006 | Self-Written LWIP-Style Network Stack                 | Accepted |
| 0007 | Industrial Demo — Sensor → MQTT-SN over UDP           | Accepted |
| 0008 | Fallback Strategy When `linkme` Conflicts With `cortex-m-rt` | Accepted |
| 0009 | Minimal TCP for M2 — RFC 793 + Simple Timeout        | Accepted |
| 0010 | Dynamic Module Loader Container Formats (M3)          | Accepted |
| 0011 | Cortex-A9 Real MMU + L1 Section Tables (M3)          | Accepted |
| 0012 | SMP Multi-Core Bring-Up (M3)                          | Accepted |
| 0013 | Loader Observability — Atomic Counters + Audit Ring   | Accepted |

## Conventions

- ADRs are immutable once accepted. A new ADR that supersedes
  an existing one links back via `Supersedes:`.
- Each ADR follows the [MADR](https://adr.github.io/madr/)
  template: Context, Decision, Consequences, Alternatives.
- ADRs are referenced from the relevant module-level docs
  (e.g. `caplet-cap/README.md` cites ADR-0001).