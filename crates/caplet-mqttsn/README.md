# `caplet-mqttsn`

Minimal MQTT-SN v1.2 client for sensor networks.

## Status

Phase 8.1 complete. Decoding + encoding for SEARCHGW /
CONNECT / REGISTER / PUBLISH / DISCONNECT, plus
CONNACK / REGACK / PUBLISH / PINGRESP / DISCONNECT parsers.

## End-to-end tests

`e2e.rs` provides a mock broker that walks the client through
the SEARCHGW → CONNECT → REGISTER → PUBLISH → DISCONNECT
state machine and asserts the published payload appears at the
broker.

## Hardware

Wire `caplet-net::syscall_net` into the MQTT-SN client via the
`NetProvider` indirection. The industrial demo uses this with
the loopback interface.