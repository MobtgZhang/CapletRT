#!/usr/bin/env bash
# Strict end-to-end test for industrial-demo + MQTT-SN.
#
# The in-process gateway consumes the actual MQTT-SN wire packets emitted by
# the client state machine. The assertions cover the complete
# SEARCHGW/CONNECT/REGISTER/PUBLISH/PUBACK/DISCONNECT exchange.

set -euo pipefail

cd "$(dirname "$0")/../.."

echo "[industrial-e2e] building and running under QEMU..."
SIM_TIMEOUT="${SIM_TIMEOUT:-10}" tools/sim/run_qemu.sh industrial-demo

log="target/sim/industrial-demo/qemu.log"
[[ -s "$log" ]] || { echo "missing QEMU log: $log" >&2; exit 1; }

fail() {
    echo "[industrial-e2e] FAIL: $*" >&2
    exit 1
}

contains() {
    grep -Fq -- "$1" "$log" || fail "missing: $1"
}

count_is() {
    local pattern="$1" expected="$2" actual
    actual=$(grep -Fc -- "$pattern" "$log" || true)
    [[ "$actual" -eq "$expected" ]] || \
        fail "expected $expected occurrences of '$pattern', found $actual"
}

contains "TX/RX expected=SEARCHGW type=SEARCHGW"
contains "TX/RX expected=GWINFO type=GWINFO"
contains "TX/RX expected=CONNECT type=CONNECT"
contains "TX/RX expected=CONNACK type=CONNACK"
contains "TX/RX expected=REGISTER type=REGISTER"
contains "TX/RX expected=REGACK type=REGACK"
contains "ACTIVE topic=sensors/temp topic_id=0x1234"
contains "PUBLISH topic=sensors/temp qos=1 payload=mock-temp&t=22.50 bytes=17 acknowledged=true"
contains "wire=180c2012340002"
contains "TX/RX expected=DISCONNECT type=DISCONNECT"
contains "UPLOAD complete=5 packets=9"
contains "STORAGE persisted seq=5"
contains "WATCHDOG healthy heartbeat=8 writes="

count_is "TX/RX expected=PUBLISH type=PUBLISH" 5
count_is "TX/RX expected=PUBACK type=PUBACK" 5
count_is "acknowledged=true" 5

if grep -Fq "WARNING: no storage writes or uploads" "$log"; then
    fail "watchdog reported an unhealthy system"
fi

echo "[industrial-e2e] PASS: 5 QoS1 publishes, 5 PUBACKs, storage and watchdog healthy"
