# Industrial Demo Walkthrough

The `industrial-demo` board is the canonical M2 demo. It models
a small remote telemetry unit that samples a temperature
sensor and publishes the readings to an MQTT-SN gateway.

## Architecture

```
+-------------------+       +----------------------+
|  sample_task      |       |  upload_task         |
|  (every 100 ms)   |       |  (driven by mailbox) |
|                   |       |                      |
|  SENSOR.read() --+---mailbox--> mqttsn.publish()   |
+-------------------+       +----------------------+
        |                              |
        v                              v
   caplet-sensors::MockSensor     caplet-mqttsn::Client
                                   |
                                   v
                          caplet-net (UDP)
                                   |
                                   v
                          Mock broker (in proc)
```

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

The shell script:

1. Builds the demo.
2. Launches QEMU with semihosting.
3. Filters semihosting output for the `PUBLISH temp=...` line
   produced by `upload_task`.

## CI

The `qemu-industrial-e2e` GitHub Actions job runs the script
above on every PR. A failed run indicates either:

- The mock broker cannot decode the published message, or
- The semaphore / mailbox plumbing is broken.

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