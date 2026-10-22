# `industrial-demo`

The canonical M2 industrial telemetry demo, runnable on the MPS2-AN500
(Cortex-M4F) model under QEMU and Renode, and on RISC-V32/RISC-V64 in the
simulation matrix.

## Tasks

| Task            | Priority | Role                                              |
|-----------------|----------|---------------------------------------------------|
| `watchdog_task` | 0        | Reads health counters, warns on stalled progress. |
| `upload_task`   | 1        | MQTT-SN QoS 1 publish loop over `MockBroker`.     |
| `storage_task`  | 2        | Validates and appends checksummed records.        |
| `sample_task`   | 3        | Reads `MockSensor` and fans out to both mailboxes.|

The storage and upload paths have separate mailboxes. The firmware prints
the MQTT-SN packet log, five `PUBLISH`/`PUBACK` pairs, a `UPLOAD complete`
line, `STORAGE persisted ... [ram-backed]` lines, and watchdog health.

## Storage limit

The MPS2 model has no persistent flash controller. Records live in a
bounded RAM store; the demo does not claim power-loss persistence.

## Gateway limit

`MockBroker` is an in-process parser/responder used by the E2E test. It
validates the real wire bytes but is not `rumqttd` and does not use a host
TCP stack.

## Run

```bash
tools/sim/run_qemu.sh industrial-demo
tools/sim/run_renode.sh industrial-demo
tests/industrial-e2e/run.sh
```

See `docs/industrial-demo.md` for the packet sequence and assertions.
