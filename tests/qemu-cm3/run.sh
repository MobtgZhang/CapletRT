#!/usr/bin/env bash
# QEMU Cortex-M3 integration test (MPS2-AN385).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/tools/sim/run_qemu.sh" cm3
