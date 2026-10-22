#!/usr/bin/env bash
# Compatibility entry point for the industrial end-to-end test.
#
# This runs the real QEMU image and validates the MQTT-SN wire exchange,
# PUBACKs, storage writes and watchdog heartbeats.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tests/industrial-e2e/run.sh"
