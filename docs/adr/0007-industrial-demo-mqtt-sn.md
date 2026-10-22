# ADR-007: Industrial Demo — Sensor → MQTT-SN over UDP

Status: Accepted
Date: 2026-07-09
Supersedes: —

## Context

The M2 milestone must show a credible end-to-end industrial
scenario. Two cooperating tasks:

1. A **sample task** that polls a temperature sensor every
   100 ms and writes the value into a mailbox.
2. An **upload task** that pops the mailbox and publishes a
   `temp` topic via MQTT-SN over UDP.

The demo must run end-to-end in QEMU without depending on
external infrastructure, so the broker is a mock embedded in
`caplet-mqttsn::e2e`.

## Decision

- **Sensor abstraction** (`caplet-sensors`) — a `Sensor` trait
  with `MockSensor` and `Tmp102`. The demo uses `MockSensor`
  for determinism; `Tmp102` is included for hardware parity.
- **Transport** — MQTT-SN v1.2 over UDP, exposed via
  `caplet-mqttsn`. SEARCHGW → CONNECT → REGISTER → PUBLISH →
  DISCONNECT, with retries on the standard timeouts.
- **Concurrency** — the two tasks run as `#[caplet::task]`
  futures; communication goes through `caplet-sync::Channel`.
- **Validation** — `tests/industrial-e2e/run.sh` builds the
  demo, runs it under QEMU with semihosting, and asserts that
  the `PUBLISH` line reaches the mock broker.

## Consequences

- The CI `qemu-industrial-e2e` job runs the demo end-to-end.
- Replacing `MockSensor` with `Tmp102` on real hardware does
  not change the application code.
- Resource budgets (Phase 4) can be tuned for each task
  without touching the demo logic.

## Alternatives considered

- **HTTP/REST instead of MQTT-SN**: rejected — too heavy for
  Cortex-M; the demo must run on QEMU with no TLS.
- **Single-task polling**: rejected — does not exercise the
  scheduler; we want the demo to show async cooperation.