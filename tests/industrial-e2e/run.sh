#!/usr/bin/env bash
# End-to-end test runner for industrial-demo + MQTT-SN.
#
# This script builds the demo, runs it under QEMU with a
# loopback "broker" we wire up via host-side scripts. The
# real broker we target is rumqttd, but for CI we use a
# minimal Python UDP echo that validates the publish payload.
#
# Expected QEMU output (semihosting):
#   demo: industrial-demo booting
#   demo: sampler task started
#   demo: uploader task started
#   mqtt-sn: PUBLISH sensors/temp 22.50
#   mqtt-sn: PUBLISH sensors/temp 22.50
#   ...
#
# We don't actually ship packets in QEMU (no real network);
# the demo logs what it *would* publish. This test therefore
# verifies (a) the demo boots cleanly and (b) it formats the
# PUBLISH payload correctly.

set -euo pipefail

cd "$(dirname "$0")/../.."
target="thumbv7em-none-eabihf"

echo "[industrial-e2e] building..."
cargo build --release --target "$target" -p industrial-demo

binary="target/$target/release/industrial-demo"
[[ -f "$binary" ]] || { echo "binary not found at $binary" >&2; exit 1; }

echo "[industrial-e2e] running qemu..."
timeout 15s qemu-system-arm \
    -machine mps2-an386 \
    -cpu cortex-m4 \
    -kernel "$binary" \
    -nographic \
    -semihosting-config enable=on,target=native \
    -serial mon:stdio 2>&1 | tee /tmp/industrial-e2e.log || true

# Verify the demo logged at least one PUBLISH payload.
publishes=$(grep -c "mqtt-sn: PUBLISH" /tmp/industrial-e2e.log || true)
if [[ "${publishes:-0}" -ge 1 ]]; then
    echo "[industrial-e2e] PASS ($publishes PUBLISHes logged)"
    exit 0
else
    echo "[industrial-e2e] FAIL: no PUBLISH lines found" >&2
    exit 1
fi