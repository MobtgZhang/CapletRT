#!/usr/bin/env bash
# QEMU Allwinner A20 (Cortex-A8 on vexpress-a9) integration test.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/sim/run_qemu.sh" a20
