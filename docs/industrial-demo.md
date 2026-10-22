# Industrial Demo Walkthrough

The `industrial-demo` board is the canonical M2 demo. It models a small
remote telemetry unit that samples a temperature sensor, records each
reading, publishes five readings over MQTT-SN, and monitors its own health.

The demo runs in QEMU and Renode on the MPS2-AN500 (Cortex-M4F) model, and
it also builds for RISC-V32/RISC-V64 in the simulation matrix.

## Architecture

```
                 +---------------------+
                 |  sample_task (prio 3)|
                 |  MockSensor.read()  |
                 +----------+----------+
                            |
              +-------------+-------------+
              v                           v
   STORAGE_LATEST mailbox        UPLOAD_LATEST mailbox
              |                           |
              v                           v
  +---------------------+      +----------------------+
  | storage_task (prio 2)|      | upload_task (prio 1) |
  | checksum + record    |      | MQTT-SN client        |
  | store [ram-backed]   |      | in-process MockBroker |
  +---------------------+      +----------------------+
                            |
                            v
                 +----------------------+
                 | watchdog_task (prio 0)|
                 | counters only        |
                 +----------------------+
```

The storage and upload mailboxes are independent so neither consumer can
steal every reading from the other. Priorities are declared on the tasks;
the watchdog holds no storage or socket token.

## Wire behaviour

The upload task performs the real MQTT-SN state machine over the packet
builder in `caplet-mqttsn`:

`SEARCHGW -> GWINFO -> CONNECT -> CONNACK -> REGISTER -> REGACK ->
PUBLISH -> PUBACK (x5) -> DISCONNECT`

Each PUBLISH uses QoS 1, topic `sensors/temp`, topic id `0x1234`, and the
compact payload `mock-temp&t=NN.NN`. The PUBACK message id is checked, and
the demo fails if the transaction is still pending afterwards.

`MockBroker` is an **in-process gateway** used by the E2E test. It parses
and answers the bytes produced by the client, so the test exercises wire
encoding and state transitions, but it is **not** a live `rumqttd` broker and
does not traverse a host network stack.

## Storage behaviour

The MPS2 model has no persistent flash controller. Records are kept in a
bounded RAM store (`RECORD_LOG`) with the same sequence/timestamp/value
checksum path a flash backend would use. Logs are tagged `[ram-backed]`;
**no power-loss persistence is claimed**. A production board can swap the
backend for `caplet-ota::FlashStorage` without changing the task or
capability boundaries.

## Source files

- `boards/industrial-demo/src/main.rs` — entry, two tasks, and
  the global `SENSOR` / `MAILBOX` statics.
- `crates/caplet-sensors/src/lib.rs` — `MockSensor` and
  `Tmp102`.
- `crates/caplet-mqttsn/src/lib.rs` — MQTT-SN client.
- `crates/caplet-mqttsn/src/e2e.rs` — the mock broker used by
  the e2e test.

## Build & run

```bash
cargo build -p industrial-demo --target thumbv7em-none-eabihf
tests/industrial-e2e/run.sh
```

The strict script (`tests/industrial-e2e/run.sh`) runs the demo through
`tools/sim/run_qemu.sh industrial-demo` and asserts:

- every MQTT-SN packet type appears in the transmit/receive log,
- exactly five `PUBLISH` and five `PUBACK` events are present,
- `UPLOAD complete=5 packets=9`,
- at least one `STORAGE persisted` record,
- the watchdog reports a healthy heartbeat,
- the raw QoS 1 PUBLISH header (`wire=180c2012340002`) is present.

## CI

The `industrial-e2e` CI job runs the script above on every PR. A failed run
indicates either:

- The in-process gateway cannot decode the published message, or
- The mailbox / priority / capability plumbing is broken.

## Replacing `MockSensor` with `Tmp102`

In production:

```rust
static I2C: SpinLock<MyI2c> = SpinLock::new(MyI2c::new());
static SENSOR: Tmp102<MyI2c> = Tmp102::new(&I2C);
```

The rest of `sample_task` is unchanged.

## Resource budgets

Each task declares a budget at spawn time:

```rust
spawn(sample_task().with_budget(Budget {
    cpu:    Some(CpuBudget    { window: 1_000, slice: 200 }),
    memory: Some(MemBudget    { bytes:  4_096 }),
    msg:    Some(MsgBudget    { window: 1_000, max:   100 }),
}));
```

Exceeding the CPU budget causes the task to be demoted for one
window. Exceeding the memory budget panics with
`BudgetExceeded(Memory)`. Exceeding the message budget publishes
a `BudgetExceeded(Message)` event into the audit log.

See ADR-0004 for the rationale.
