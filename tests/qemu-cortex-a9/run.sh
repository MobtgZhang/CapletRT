#!/usr/bin/env bash
# QEMU Cortex-A9 integration test.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/sim/run_qemu.sh cortex-a9"
